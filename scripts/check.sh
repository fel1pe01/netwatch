#!/usr/bin/env bash
# scripts/check.sh — Puerta de validación previa al push (réplica local de la CI).
#
# Por defecto: cargo clean y, desde cero, fmt, check, test, doc-test, clippy,
# build release, cargo audit, cargo deny, E2E smoke y build de Docker.
# No se detiene en el primer fallo (salvo --fail-fast) e imprime un resumen.
#
# Los comandos cargo replican los de .github/workflows/ci.yml (incluido --locked
# y --all-features). Si cambias uno, cambia el otro.
#
# Códigos de salida:
#   0  Validación completa y correcta: apto para push.
#   1  Algún paso falló.
#   2  No se pudo empezar (sin espacio, otro cargo en marcha, etc.).
#   3  Sin fallos, pero la validación es INCOMPLETA (modo parcial, herramienta
#      ausente, working tree sucio...). No cuenta como apto para push.
set -uo pipefail

usage() {
  cat <<'EOF'
Uso: scripts/check.sh [opciones]

Sin opciones ejecuta la validación completa desde cero (la que debe pasar
antes de hacer push).

Modos parciales (el resultado será INCOMPLETO, código de salida 3):
  --quick         Solo fmt, check, test y clippy, sin cargo clean (iterar rápido)
  --no-clean      Omite el cargo clean inicial

Omisiones explícitas (se aceptan y se listan en el resumen, salida 0):
  --skip-docker   No construye la imagen Docker (p. ej. sin Docker en WSL)
  --skip-audit    No ejecuta cargo audit ni cargo deny
  --skip-e2e      No ejecuta el smoke test E2E

Otras:
  --fail-fast     Se detiene en el primer paso que falle
  --clean-after   Ejecuta cargo clean al terminar
  --allow-dirty   Permite working tree con cambios sin commitear
  -h, --help      Muestra esta ayuda

Variables de entorno:
  MIN_FREE_GB=10        Espacio libre mínimo para empezar
  CARGO_BUILD_JOBS=2    Jobs de cargo (por defecto 2, para limitar RAM)
  AUDIT_IGNORE="ID ..." Advisories a ignorar, p. ej. "RUSTSEC-2023-0071"
  KNOWN_FAILURES_FILE   Tests fallidos conocidos (scripts/known-failures.txt)
  E2E_TIMEOUT=300       Segundos máximos para el smoke test E2E

Los advisories a ignorar deben ir en .cargo/audit.toml (lo leen este script y
la CI). AUDIT_IGNORE solo existe en local y vuelve el resultado INCOMPLETO.
EOF
}

QUICK=0; NO_CLEAN=0; FAIL_FAST=0; CLEAN_AFTER=0; ALLOW_DIRTY=0
SKIP_E2E=0; SKIP_DOCKER=0; SKIP_AUDIT=0
for arg in "$@"; do
  case "$arg" in
    --quick) QUICK=1; NO_CLEAN=1 ;;
    --no-clean) NO_CLEAN=1 ;;
    --skip-docker) SKIP_DOCKER=1 ;;
    --skip-audit) SKIP_AUDIT=1 ;;
    --skip-e2e) SKIP_E2E=1 ;;
    --fail-fast) FAIL_FAST=1 ;;
    --clean-after) CLEAN_AFTER=1 ;;
    --allow-dirty) ALLOW_DIRTY=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Opción desconocida: $arg" >&2; usage >&2; exit 2 ;;
  esac
done

# --- colores (solo si la salida es un terminal) ------------------------------
if [ -t 1 ]; then
  RED=$'\033[31m'; GREEN=$'\033[32m'; YELLOW=$'\033[33m'; BOLD=$'\033[1m'; DIM=$'\033[2m'; RESET=$'\033[0m'
else
  RED=""; GREEN=""; YELLOW=""; BOLD=""; DIM=""; RESET=""
fi
info() { printf '%s\n' "$*"; }
warn() { printf '%s%s%s\n' "$YELLOW" "$*" "$RESET"; }
err()  { printf '%s%s%s\n' "$RED" "$*" "$RESET" >&2; }

# --- entorno ------------------------------------------------------------------
ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || { err "Esto no es un repositorio git."; exit 2; }
cd "$ROOT" || exit 2
[ -f Cargo.toml ] || { err "No se encontró Cargo.toml en $ROOT"; exit 2; }

MIN_FREE_GB=${MIN_FREE_GB:-10}
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
KNOWN_FAILURES_FILE=${KNOWN_FAILURES_FILE:-scripts/known-failures.txt}
AUDIT_IGNORE=${AUDIT_IGNORE:-}
E2E_TIMEOUT=${E2E_TIMEOUT:-300}

# Evita dos ejecuciones simultáneas y cargo concurrente sobre el mismo target/.
if command -v flock >/dev/null 2>&1; then
  exec 9>"${TMPDIR:-/tmp}/netwatch-check.lock"
  flock -n 9 || { err "Ya hay otra ejecución de check.sh en curso."; exit 2; }
fi
if pgrep -x cargo >/dev/null 2>&1; then
  err "Hay otro proceso cargo en ejecución. Espera a que termine (evita corromper target/)."
  exit 2
fi

kb_to_gib() { awk -v k="$1" 'BEGIN { printf "%.1f GiB", k / 1048576 }'; }
free_kb=$(df -Pk . | awk 'NR==2 {print $4}')
if [ $((free_kb / 1048576)) -lt "$MIN_FREE_GB" ]; then
  err "Solo hay $(kb_to_gib "$free_kb") libres; se requieren al menos ${MIN_FREE_GB} GiB."
  err "Libera espacio (cargo clean, compactar el .vhdx) o baja MIN_FREE_GB bajo tu responsabilidad."
  exit 2
fi
target_kb() { du -sk target 2>/dev/null | awk '{print $1}' | grep -E '^[0-9]+$' || echo 0; }
TARGET_START_KB=$(target_kb)

LOG_DIR=$(mktemp -d "${TMPDIR:-/tmp}/netwatch-check.XXXXXX")
BRANCH=$(git branch --show-current 2>/dev/null)
HEAD_SHA=$(git rev-parse HEAD 2>/dev/null)
DIRTY=$(git status --short | wc -l | tr -d ' ')

# --- registro de resultados ---------------------------------------------------
NAMES=(); RESULTS=(); DURS=(); NOTES=()
REASONS=()    # motivos por los que la validación es INCOMPLETA
ACCEPTED=()   # omisiones explícitas aceptadas por el usuario
ANY_FAIL=0; CHECK_FAILED=0; RELEASE_FAILED=0

record()   { NAMES+=("$1"); RESULTS+=("$2"); DURS+=("$3"); NOTES+=("${4:-}"); }
set_note() { NOTES[${#NOTES[@]}-1]="$1"; }
skip_explicit() { record "$1" SKIP 0 "$2"; ACCEPTED+=("$1: $2"); }
missing()  { record "$1" FALTA 0 "$2"; REASONS+=("$1: $2"); }

[ "$QUICK" -eq 1 ] && REASONS+=("modo --quick: no ejecuta doc-test, release, audit, deny, e2e ni docker")
[ "$NO_CLEAN" -eq 1 ] && REASONS+=("sin cargo clean inicial: no equivale a una CI desde cero")
if [ -n "$AUDIT_IGNORE" ] && [ "$SKIP_AUDIT" -eq 0 ] && [ "$QUICK" -eq 0 ]; then
  REASONS+=("AUDIT_IGNORE solo existe en local: la CI no lo ve; mueve los ignores a .cargo/audit.toml")
fi
if [ "$DIRTY" -ne 0 ]; then
  if [ "$ALLOW_DIRTY" -eq 1 ]; then
    ACCEPTED+=("working tree: $DIRTY cambio(s) sin commitear (--allow-dirty)")
  else
    REASONS+=("working tree con $DIRTY cambio(s) sin commitear: el resultado no corresponde solo al SHA y un archivo sin 'git add' pasaría aquí pero fallaría en la CI")
  fi
fi

# Archivos que la CI necesita deben estar en git: uno sin 'git add' pasaría aquí
# (con --allow-dirty) pero fallaría en la CI, que parte de un checkout limpio.
for f in Cargo.lock .cargo/deny.toml Dockerfile .cargo/audit.toml .github/workflows/ci.yml; do
  if [ -e "$f" ] && ! git ls-files --error-unmatch "$f" >/dev/null 2>&1; then
    REASONS+=("$f existe pero no está en git (git add): la CI no lo verá")
  fi
done
CI_FILE=.github/workflows/ci.yml
CI_TOOLCHAIN=""
if [ ! -f "$CI_FILE" ]; then
  REASONS+=("no existe $CI_FILE: no hay workflow con el que comparar")
else
  CI_TOOLCHAIN=$(sed -n 's/^[[:space:]]*RUST_TOOLCHAIN:[[:space:]]*"\{0,1\}\([^"# ]*\).*/\1/p' "$CI_FILE" | head -n 1)
fi
RUSTC_VERSION=$(rustc --version 2>/dev/null | awk '{print $2}')
case "$CI_TOOLCHAIN" in
  "") ;;
  stable) ACCEPTED+=("toolchain: la CI usa 'stable' flotante y puede traer lints más nuevos que tu rustc ${RUSTC_VERSION:-?}; fija RUST_TOOLCHAIN") ;;
  *)
    case "$RUSTC_VERSION" in
      "$CI_TOOLCHAIN"|"$CI_TOOLCHAIN".*) ;;
      *) REASONS+=("rustc local ${RUSTC_VERSION:-no encontrado} no coincide con el toolchain de la CI ($CI_TOOLCHAIN)") ;;
    esac ;;
esac

info "${BOLD}Netwatch — validación previa al push${RESET}"
info "  Rama:        ${BRANCH:-"(detached)"}"
info "  HEAD:        $HEAD_SHA"
info "  Cambios:     $DIRTY archivo(s) sin commitear"
info "  Toolchain:   $(rustc --version 2>/dev/null || echo 'rustc no encontrado') (CI: ${CI_TOOLCHAIN:-?})"
info "  Espacio:     $(kb_to_gib "$free_kb") libres | target/ $(kb_to_gib "$TARGET_START_KB")"
info "  Jobs cargo:  $CARGO_BUILD_JOBS"
info "  Logs:        $LOG_DIR"
echo

# Pasos que no tiene sentido ejecutar si no compila.
needs_compile() {
  case "$1" in test|doc-test|clippy|release) return 0 ;; *) return 1 ;; esac
}

# step <nombre> <comando...>  → deja el código de salida en STEP_RC y el log en STEP_LOG
STEP_RC=0; STEP_LOG=""
step() {
  local name="$1"; shift
  if [ "$FAIL_FAST" -eq 1 ] && [ "$ANY_FAIL" -eq 1 ]; then
    record "$name" SKIP 0 "omitido por --fail-fast"; STEP_RC=-1; return
  fi
  if needs_compile "$name" && [ "$CHECK_FAILED" -eq 1 ]; then
    record "$name" SKIP 0 "no compila (falló check)"; STEP_RC=-1; return
  fi
  STEP_LOG="$LOG_DIR/$name.log"
  printf '%s▶ %s%s %s%s%s\n' "$BOLD" "$name" "$RESET" "$DIM" "$*" "$RESET"
  local start=$SECONDS
  "$@" >"$STEP_LOG" 2>&1
  STEP_RC=$?
  local dur=$((SECONDS - start))
  if [ "$STEP_RC" -eq 0 ]; then
    record "$name" PASS "$dur"
  else
    record "$name" FAIL "$dur"; ANY_FAIL=1
  fi
}

show_tail() { printf '%s\n' "${DIM}--- últimas líneas de $STEP_LOG ---${RESET}"; tail -n 15 "$STEP_LOG"; echo; }

# --- 0. limpieza inicial ------------------------------------------------------
if [ "$NO_CLEAN" -eq 1 ]; then
  record clean SKIP 0 "omitido"
else
  step clean cargo clean
  if [ "$STEP_RC" -eq 0 ]; then set_note "$(tail -n 1 "$STEP_LOG" | sed 's/^ *//')"; else show_tail; fi
fi

# --- 1. comprobaciones de código ---------------------------------------------
step fmt cargo fmt --all -- --check
[ "$STEP_RC" -gt 0 ] && show_tail

step check cargo check --locked --all-targets --all-features
if [ "$STEP_RC" -gt 0 ]; then CHECK_FAILED=1; show_tail; fi

# --no-fail-fast: sin él, cargo se detiene en el primer binario de tests que
# falla y los siguientes no llegan a ejecutarse.
step test cargo test --locked --all-targets --all-features --no-fail-fast
if [ "$STEP_RC" -gt 0 ]; then
  mapfile -t FAILED_TESTS < <(grep -E '^test .+ \.\.\. FAILED$' "$STEP_LOG" \
    | sed -E 's/^test (.+) \.\.\. FAILED$/\1/' | sort -u)
  if [ "${#FAILED_TESTS[@]}" -eq 0 ]; then
    show_tail   # fallo de compilación o de enlazado, no de un test concreto
    set_note "sin tests identificados: revisa el log"
  else
    NEW=0; KNOWN=0
    echo "  Tests fallidos:"
    for t in "${FAILED_TESTS[@]}"; do
      base="${t##*::}"
      if [ -f "$KNOWN_FAILURES_FILE" ] && grep -vE '^\s*(#|$)' "$KNOWN_FAILURES_FILE" | grep -qxF "$base"; then
        printf '    %s[conocido]%s %s\n' "$YELLOW" "$RESET" "$t"; KNOWN=$((KNOWN + 1))
      else
        printf '    %s[NUEVO]%s    %s\n' "$RED" "$RESET" "$t"; NEW=$((NEW + 1))
      fi
    done
    grep -E "panicked at|^  left:|^ right:|assertion" "$STEP_LOG" | sed 's/^/    /' | head -n 30
    echo
    set_note "${#FAILED_TESTS[@]} fallidos ($NEW nuevos, $KNOWN conocidos)"
  fi
fi

if [ "$QUICK" -eq 1 ]; then
  record doc-test SKIP 0 "--quick"
else
  step doc-test cargo test --locked --doc --all-features
  [ "$STEP_RC" -gt 0 ] && show_tail
fi

step clippy cargo clippy --locked --all-targets --all-features -- -D warnings
[ "$STEP_RC" -gt 0 ] && show_tail

if [ "$QUICK" -eq 1 ]; then
  record release SKIP 0 "--quick"
else
  step release cargo build --locked --release
  if [ "$STEP_RC" -gt 0 ]; then RELEASE_FAILED=1; show_tail; fi
fi

# --- 2. seguridad de dependencias --------------------------------------------
if [ "$QUICK" -eq 1 ]; then
  record audit SKIP 0 "--quick"; record deny SKIP 0 "--quick"
elif [ "$SKIP_AUDIT" -eq 1 ]; then
  skip_explicit audit "--skip-audit"; skip_explicit deny "--skip-audit"
else
  if cargo audit --version >/dev/null 2>&1; then
    AUDIT_ARGS=()
    for id in $AUDIT_IGNORE; do AUDIT_ARGS+=(--ignore "$id"); done
    step audit cargo audit ${AUDIT_ARGS[@]+"${AUDIT_ARGS[@]}"}
    if [ "$STEP_RC" -gt 0 ]; then
      echo "  Hallazgos:"
      awk 'BEGIN { RS = ""; FS = "\n" }
           /^Crate:/ {
             c = ""; v = ""; id = ""; w = ""
             for (i = 1; i <= NF; i++) {
               if ($i ~ /^Crate:/)   { sub(/^Crate: +/, "", $i);   c = $i }
               if ($i ~ /^Version:/) { sub(/^Version: +/, "", $i); v = $i }
               if ($i ~ /^ID:/)      { sub(/^ID: +/, "", $i);      id = $i }
               if ($i ~ /^Warning:/) { w = " (aviso)" }
             }
             print "    - " c " " v "  " id w
           }' "$STEP_LOG"
      echo
    fi
  else
    missing audit "falta cargo-audit: cargo install cargo-audit --locked"
  fi

  if ! cargo deny --version >/dev/null 2>&1; then
    missing deny "falta cargo-deny: cargo install cargo-deny --locked"
  elif [ ! -f .cargo/deny.toml ]; then
    missing deny "falta .cargo/deny.toml (la CI ejecuta cargo deny y fallará sin configuración)"
  else
    step deny cargo deny check advisories bans licenses sources
    [ "$STEP_RC" -gt 0 ] && show_tail
  fi
fi

# --- 3. E2E (la CI lo ejecuta como ./scripts/e2e_smoke.sh) ---------------------
if [ "$QUICK" -eq 1 ]; then
  record e2e SKIP 0 "--quick"
elif [ "$SKIP_E2E" -eq 1 ]; then
  skip_explicit e2e "--skip-e2e"
elif [ ! -f scripts/e2e_smoke.sh ]; then
  missing e2e "no existe scripts/e2e_smoke.sh"
elif [ "$CHECK_FAILED" -eq 1 ] || [ "$RELEASE_FAILED" -eq 1 ]; then
  record e2e SKIP 0 "sin binario release"
else
  # El bit de ejecución que cuenta es el registrado en git (la CI hace checkout
  # desde git); en WSL sobre /mnt/c el del disco puede no coincidir.
  E2E_MODE=$(git ls-files -s scripts/e2e_smoke.sh | awk '{print $1}')
  if [ -z "$E2E_MODE" ]; then
    missing e2e "scripts/e2e_smoke.sh no está en git (git add)"
  elif [ "$E2E_MODE" != "100755" ]; then
    record e2e FAIL 0 "modo en git $E2E_MODE: git update-index --chmod=+x scripts/e2e_smoke.sh"
    ANY_FAIL=1
  else
    # Se ejecuta directamente (no con 'bash') para detectar un bit de ejecución
    # ausente en disco. timeout evita que un servidor colgado bloquee el script.
    if command -v timeout >/dev/null 2>&1; then
      step e2e timeout "$E2E_TIMEOUT" ./scripts/e2e_smoke.sh
    else
      step e2e ./scripts/e2e_smoke.sh
    fi
    if [ "$STEP_RC" -gt 0 ]; then
      [ "$STEP_RC" -eq 124 ] && set_note "timeout tras ${E2E_TIMEOUT}s"
      show_tail
    fi
  fi
fi

# --- 4. Docker ----------------------------------------------------------------
if [ "$QUICK" -eq 1 ]; then
  record docker SKIP 0 "--quick"
elif [ "$SKIP_DOCKER" -eq 1 ]; then
  skip_explicit docker "--skip-docker"
elif [ ! -f Dockerfile ]; then
  missing docker "no existe Dockerfile"
elif ! command -v docker >/dev/null 2>&1; then
  missing docker "no se encuentra el ejecutable docker en PATH; en WSL activa la integración de Docker Desktop"
elif ! timeout 15s docker info >/dev/null 2>&1; then
  missing docker "Docker no responde en 15 segundos; comprueba Docker Desktop y la integración WSL"
else
  # Gestionar imagen netwatch:check: si existe, eliminarla y reconstruir; si no, construir directamente.
  # Usamos un subshell para que la eliminación y la construcción formen parte del mismo paso.
  step docker bash -c '
    set -euo pipefail
    if docker image inspect netwatch:check >/dev/null 2>&1; then
      echo "  Imagen netwatch:check encontrada; eliminando antes de reconstruir..."
      docker rmi netwatch:check
    fi
    docker build --pull -t netwatch:check .
  '
  if [ "$STEP_RC" -gt 0 ]; then show_tail; fi
  # Limpiar imagen de validación (best-effort, no registrado como paso)
  docker rmi netwatch:check >/dev/null 2>&1 || true
fi

# --- resumen ------------------------------------------------------------------
echo
info "${BOLD}Resumen${RESET}"
printf '  %-10s %-6s %6s  %s\n' "PASO" "ESTADO" "TIEMPO" "NOTAS"
for i in "${!NAMES[@]}"; do
  case "${RESULTS[$i]}" in
    PASS) color=$GREEN ;; FAIL) color=$RED ;; *) color=$YELLOW ;;
  esac
  printf '  %-10s %s%-6s%s %5ss  %s\n' "${NAMES[$i]}" "$color" "${RESULTS[$i]}" "$RESET" "${DURS[$i]}" "${NOTES[$i]}"
done
echo
TARGET_END_KB=$(target_kb)
info "  SHA:       $HEAD_SHA"
info "  target/:   $(kb_to_gib "$TARGET_END_KB")"
info "  Logs:      $LOG_DIR"

if [ "$CLEAN_AFTER" -eq 1 ]; then
  echo
  info "${BOLD}Limpieza final${RESET}"
  cargo clean 2>&1 | tail -n 1
  info "  Espacio libre ahora: $(kb_to_gib "$(df -Pk . | awk 'NR==2 {print $4}')")"
  info "  Recuerda compactar el .vhdx desde Windows para recuperar el espacio en C:."
fi

echo
if [ "$ANY_FAIL" -ne 0 ]; then
  err "RESULTADO: FALLO — no hagas push hasta resolver los pasos en rojo."
  exit 1
fi
if [ "${#REASONS[@]}" -ne 0 ]; then
  warn "RESULTADO: INCOMPLETO — sin fallos, pero NO es una validación completa:"
  for r in "${REASONS[@]}"; do warn "  - $r"; done
  exit 3
fi
info "${GREEN}${BOLD}RESULTADO: APTO PARA PUSH — validación completa desde cero sobre $HEAD_SHA${RESET}"
if [ "${#ACCEPTED[@]}" -ne 0 ]; then
  warn "Aceptado sin validar localmente (lo cubrirá la CI):"
  for a in "${ACCEPTED[@]}"; do warn "  - $a"; done
fi
exit 0