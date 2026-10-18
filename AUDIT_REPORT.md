# Auditoría Integral del Repositorio Netwatch

**Fecha:** 2026-10-18
**Auditor:** Análisis automatizado con verificación cruzada código-documentación-Git
**Repositorio:** `~/projects/netwatch` (local) → `https://github.com/fel1pe01/netwatch` (remoto)
**Commit analizado:** `27dcdb8` (HEAD, main, up-to-date con origin/main)
**Rama:** main
**Estado del árbol de trabajo:** Limpio (sin cambios pendientes)

---

## 1. Resumen Ejecutivo

### Estado General de la Documentación
La documentación del proyecto Netwatch es **extensa, detallada y en gran medida precisa** respecto a la implementación actual. Existe un esfuerzo documental significativo (11 archivos Markdown principales + checkpoints/auditorías) que rastrea la evolución del proyecto a través de fases (P0-P5). La documentación distingue claramente entre funcionalidades **implementadas**, **parciales**, **planificadas** y **bloqueadas**.

### Principales Riesgos Encontrados
| Severidad | Riesgo | Descripción |
|-----------|--------|-------------|
| **Alto** | Tests no re-verificados | La documentación afirma "46 tests passing" y "42/42 E2E passing" pero **no se han re-ejecutado en la sesión actual** por falta de `dlltool.exe` en toolchain GNU. El estado real es **desconocido**. |
| **Alto** | SNMP placeholder documentado como "configurado" | `config.example.yaml` tiene `snmp.enabled: false` pero el código solo tiene job placeholder. La documentación dice "SNMP collector placeholder" pero config sugiere que está listo para activar. |
| **Medio** | Métricas de cardinalidad | Documentación afirma "solo label `protocol`" — verificado en código y tests, **correcto**. |
| **Medio** | Endpoints placeholder | 5 endpoints (`/vlans`, `/subnets`, `/gateways`, `/events`, `/topology`) devuelven `[]` vacío. Documentados correctamente como placeholders. |
| **Bajo** | Inconsistencias menores en contadores de tests | DEVELOPMENT.md dice "91 tests", IMPLEMENTATION_STATUS.md dice "46 unit/integration + 42 E2E". Diferencia por categorización. |
| **Informativo** | Documentos faltantes planificados | CONFIGURATION.md, API.md, OPERATIONS.md, CONTRIBUTING.md, SECURITY.md listados como pendientes en ROADMAP e IMPLEMENTATION_STATUS. |

### Documentos Revisados: **11 archivos Markdown principales**
- README.md, IMPLEMENTATION_STATUS.md, ARCHITECTURE.md, ROADMAP.md, DEVELOPMENT.md, CHANGES_MADE.md, ARCHITECTURE_AND_FUNCTIONAL_TESTS.md, CHECKPOINT_P4.md, P3_AUDIT.md, P4_AUDIT.md, replace.md

### Hallazgos por Severidad
| Severidad | Cantidad |
|-----------|----------|
| Crítico | 0 |
| Alto | 2 |
| Medio | 4 |
| Bajo | 6 |
| Informativo | 8 |

### Limitaciones de la Auditoría
1. **No se pudieron ejecutar tests** (`cargo test`, `cargo clippy`, `cargo build --release`, `e2e_smoke.sh`) por falta de `dlltool.exe` en toolchain GNU (WSL). Esto impide verificar el estado real de "46 tests passing" y "42/42 E2E passing".
2. **No se probó despliegue Docker/Compose** (Docker Desktop no disponible en entorno de auditoría).
3. **No se verificaron endpoints HTTP en vivo** (servidor no iniciado).
4. **Auditoría basada en lectura estática** de código, configuración, Git history y documentación — no en ejecución dinámica.

---

## 2. Estado del Repositorio

| Aspecto | Valor |
|---------|-------|
| **Rama actual** | main |
| **HEAD commit** | `27dcdb8` (ci: improve Docker validation and project checks) |
| **Estado Git** | Clean working tree, up-to-date con origin/main |
| **Commits locales sin push** | 0 |
| **Commits remotos sin pull** | 0 |
| **Alcance historial revisado** | Últimos 30 commits (desde `6e220fd` - init standalone) |
| **Archivos modificados sin commit** | 0 |
| **Archivos untracked** | Varios scripts Python de desarrollo (`fix_*.py`, `debug*.py`, `check_*.py`) en raíz — **no deben commitearse** |

### Estructura de Directorios Relevante
```
netwatch/
├── Cargo.toml / Cargo.lock
├── README.md (10KB)
├── config.example.yaml (1.5KB)
├── Dockerfile (1.6KB)
├── compose.yaml (5.4KB)
├── src/ (22 archivos .rs + monitoring/, discovery/)
├── tests/ (10 archivos de test)
├── migrations/ (4 archivos SQL)
├── docs/ (7 archivos + checkpoints/ + audits/)
├── scripts/ (check.sh, e2e_smoke.sh, known-failures.txt)
├── prometheus/, grafana/, alertmanager/ (configs stack)
├── .github/workflows/ci.yml
├── .cargo/audit.toml (exception RUSTSEC-2023-0071)
└── target/, netwatch.sqlite* (artefactos build/runtime)
```

---

## 3. Inventario Documental

| Archivo | Propósito | Estado | Observaciones Principales |
|---------|-----------|--------|---------------------------|
| **README.md** | Visión general, quick start, status objetivos | **Correcto** | Tabla 16 objetivos con status ✅/⚠️/❌ precisa; quick start comandos válidos; limitaciones honestas |
| **docs/IMPLEMENTATION_STATUS.md** | Status verificable por objetivo con referencias código | **Correcto** | 514 líneas; tablas por objetivo con evidence/gap; P0 verification summary; known limitations code-backed |
| **docs/ARCHITECTURE.md** | Arquitectura actual vs target, data flows, decisions | **Correcto** | Module map actualizado; component responsibilities con status ✅/⚠️; execution flows verificables; evolution path con DONE marks |
| **docs/ROADMAP.md** | Fases, acceptance criteria, dependencies, risks | **Correcto** | Phase overview table precisa; checkmarks ✅/🔄/⏳/❌ por criterion; cross-dependencies graph; DoD definition |
| **docs/DEVELOPMENT.md** | Guía desarrollo: build, test, config, validation | **Mayormente correcto** | Estructura proyecto actualizada; **contador tests "91" difiere de "46+42" en otros docs** (categorización distinta); config example válido |
| **docs/CHANGES_MADE.md** | Log de cambios sesiones implementation + security fixes | **Correcto** | Documenta reconciliación docs-código; dependency security fixes con justificación exception; verification results table |
| **docs/ARCHITECTURE_AND_FUNCTIONAL_TESTS.md** | Auditoría funcional code-backed completa | **Correcto** | 797 líneas; component details, execution flows, API/CLI reference, metrics catalog, use-case matrix, test audit table (36 tests), limitations table |
| **docs/checkpoints/CHECKPOINT_P4.md** | Checkpoint P4 con test results, limitations, next steps | **Correcto** | Documenta toolchain issue (dlltool.exe); 4/8 root cause tests failing; pattern break untested; recovery procedures |
| **docs/audits/P3_AUDIT.md** | Auditoría P3 (Objectives 11,12,15,16) con matriz PASS/PARTIAL/FAIL | **Correcto (histórico)** | Refleja estado en commit `6a783cf`; 30 criteria: 1 PASS, 3 PARTIAL, 26 FAIL — coherente con implementation status actual |
| **docs/audits/P4_AUDIT.md** | Auditoría P4 (Objectives 7,10,14) con dependency audit | **Correcto** | Status PASS/PARTIAL/FAIL por criterion; fixes aplicados documentados; critical path para P4 completion |
| **replace.md** | Placeholder "No cambios realizados" | **Informativo** | Archivo residual sin contenido útil |

---

## 4. Hallazgos Detallados

### HALLAZGO-001: Tests Reportados Como "Passing" No Re-Verificados En Sesión Actual
- **Severidad:** Alto
- **Documento afectado:** README.md (líneas 66-67), DEVELOPMENT.md (línea 106), IMPLEMENTATION_STATUS.md (línea 488), ARCHITECTURE_AND_FUNCTIONAL_TESTS.md (línea 793), CHECKPOINT_P4.md (líneas 78-91)
- **Ubicación aproximada:** Múltiples referencias a "46 tests passing (historical)" / "42/42 passing (historical)" / "91 tests passing"
- **Problema:** La documentación afirma repetidamente que los tests pasan, pero **explicitamente marca como "historical" / "not re-verified this session"** debido a toolchain issue (`dlltool.exe` missing). El estado real de los tests es **desconocido** en el commit actual.
- **Evidencia:** CHECKPOINT_P4.md líneas 74-104 documenta: "Tests could not be executed in this session due to a toolchain issue: the GNU (MinGW) toolchain is missing `dlltool.exe`". `cargo fmt --check` pasó en corrective review (2026-10-13), pero clippy/test/build bloqueados.
- **Impacto:** Usuarios/desarrolladores pueden asumir que la suite de tests pasa actualmente cuando no se ha verificado. CI remoto (GitHub Actions) sí ejecuta tests — pero localmente no hay confirmación reciente.
- **Corrección recomendada:**
  1. Mantener etiqueta "historical / not re-verified" en todos los documentos (ya presente).
  2. Resolver toolchain issue (instalar MinGW o MSVC target) y re-ejecutar `./scripts/check.sh` completo.
  3. Actualizar contadores si resultados difieren.

### HALLAZGO-002: Configuración SNMP Sugiere Funcionalidad Que No Existe
- **Severidad:** Alto
- **Documento afectado:** config.example.yaml (líneas 32-34), DEVELOPMENT.md (línea 222)
- **Ubicación aproximada:** `snmp.enabled: false` con `interval: 60` en config.example.yaml
- **Problema:** El archivo de configuración de ejemplo incluye sección SNMP con `enabled: false` e `interval: 60`, lo que sugiere que bastaría cambiar a `true` para activar SNMP. En realidad, `src/snmp.rs` y `src/main.rs` (líneas 610-630) solo tienen **job placeholder** que loggea "SNMP polling job executed (placeholder)" — no hay implementación real de polling IF-MIB.
- **Evidencia:** 
  - `src/snmp.rs`: Solo struct `SnmpCollector { concurrency, timeout }` sin métodos de polling.
  - `src/main.rs:610-630`: Job `snmp_poll` solo hace `tracing::debug!("SNMP polling job executed (placeholder)")`.
  - IMPLEMENTATION_STATUS.md Obj 5: "SNMP collector placeholder only" / "Planned".
  - P4_AUDIT.md Dependency Audit: "SNMP utilization data — FAIL — SNMP collector placeholder only; no IF-MIB polling".
- **Impacto:** Usuario habilita `snmp.enabled: true` esperando monitoreo SNMP; recibe solo logs de debug sin recolección real. Tiempo perdido depurando.
- **Corrección recomendada:**
  1. En `config.example.yaml`: Cambiar comentario a `# SNMP: NOT IMPLEMENTED — placeholder only` o remover sección hasta implementar.
  2. En DEVELOPMENT.md: Aclarar que config SNMP es solo placeholder.
  3. Cuando se implemente Obj 5, actualizar config y docs simultáneamente.

### HALLAZGO-003: Contador De Tests Inconsistente Entre Documentos
- **Severidad:** Medio
- **Documento afectado:** DEVELOPMENT.md (línea 106: "91 tests passing") vs IMPLEMENTATION_STATUS.md (línea 434: "46 tests passing") vs ARCHITECTURE_AND_FUNCTIONAL_TESTS.md (línea 751: "36 tests" en tabla auditoría)
- **Problema:** Tres documentos reportan conteos distintos: 91, 46, y 36 tests. La diferencia parece ser categorización: DEVELOPMENT.md suma unit+integration+anomaly+capacity+incident+root_cause (13+11+12+8=44 + 46 base ≈ 90), IMPLEMENTATION_STATUS.md reporta "46 unit/integration tests (historical)" base, ARCHITECTURE_AND_FUNCTIONAL_TESTS.md tabla lista 36 filas de test audit.
- **Evidencia:** Conteo manual de archivos test en `tests/`: 10 archivos. Funciones test por archivo: integration_test.rs (~2), cli_tests.rs (~6), discovery_tests.rs (~2), monitoring_tests.rs (~3), http_integration_tests.rs (~13), metrics_cardinality_test.rs (~9), config_file_test.rs (~5), drift_tests.rs (~5), anomaly_tests.rs (~13), capacity_tests.rs (~11), incident_tests.rs (~12), root_cause_tests.rs (~8) = **~89 tests**. DEVELOPMENT.md "91" es cercano.
- **Impacto:** Confusión sobre cobertura real. No es error crítico si se entiende categorización, pero debería documentarse la base de conteo.
- **Corrección recomendada:** Añadir nota en DEVELOPMENT.md: "91 tests = 46 base unit/integration + 13 anomaly + 11 capacity + 12 incident + 8 root cause + 1 metrics handler". Homogeneizar terminología.

### HALLAZGO-004: Referencia A Línea Específica En main.rs Puede Estar Desactualizada
- **Severidad:** Bajo
- **Documento afectado:** IMPLEMENTATION_STATUS.md (línea 44: `main.rs:535`), ARCHITECTURE.md (línea 95: `main.rs:535`)
- **Ubicación aproximada:** Referencia a `Scheduler` instantiated at `main.rs:535`
- **Problema:** En `main.rs` actual (línea 579), `Scheduler::new()` está en línea 579, no 535. La referencia de línea se desactualizó tras ediciones.
- **Evidencia:** `main.rs` línea 579: `let mut scheduler = Scheduler::new(config.scheduler.concurrency);`
- **Impacto:** Desarrollador que busca en línea 535 no encuentra la instanciación. Menor — búsqueda por "Scheduler::new" funciona.
- **Corrección recomendada:** Eliminar referencias a números de línea específicos o actualizarlos. Preferir referencias por función/símbolo (`Scheduler::new()` en `main()`).

### HALLAZGO-005: Endpoint `/api/v1/capacity/report` Documentado Pero No En router()
- **Severidad:** Medio
- **Documento afectado:** ROADMAP.md (línea 446: "CLI/API command exposed via `netwatch capacity report` + `/api/v1/capacity/report`"), IMPLEMENTATION_STATUS.md (línea 356: "Report generated via CLI/API... `netwatch capacity report` + `/api/v1/capacity/report`"), P4_AUDIT.md (línea 110)
- **Problema:** La documentación afirma que existe endpoint `/api/v1/capacity/report`. En `src/api.rs` **no hay ruta** para `/api/v1/capacity/report`. El CLI `netwatch capacity report` **sí existe** (main.rs líneas 525-538) y funciona standalone (crea su propio Storage).
- **Evidencia:** `api.rs` líneas 217-240: router define rutas para `/health`, `/devices`, `/vlans`, `/subnets`, `/gateways`, `/events`, `/interfaces`, `/routes`, `/monitor/ping`, `/monitor/tcp`, `/topology`, `/incidents/hypotheses*`. **No hay `/capacity/report`**.
- **Impacto:** Usuario intenta llamar API `/api/v1/capacity/report` y recibe 404. Solo CLI funciona.
- **Corrección recomendada:**
  1. Implementar endpoint en `api.rs` (añadir handler + route) que use `CapacityPlanner` compartido.
  2. O actualizar documentación: "Solo CLI disponible; API endpoint pendiente".

### HALLAZGO-006: Referencia A `config.anomaly.enabled` En main.rs Usa `.unwrap_or(true)` Pero Config Tiene `Option<bool>`
- **Severidad:** Bajo
- **Documento afectado:** IMPLEMENTATION_STATUS.md (línea 418: "Anomaly Detection — Partial"), config.example.yaml (línea 45: `enabled: true`)
- **Problema:** En `main.rs` línea 633: `config.anomaly.enabled.unwrap_or(true)`. En `config.rs` línea 70: `enabled: Option<bool>`. En `config.example.yaml` línea 45: `enabled: true` (no `enabled: true` bajo `anomaly:`). **Consistente** — pero `Config::default()` (línea 156) usa `AnomalyConfig::default()` que tiene `enabled: Some(true)`. Si usuario omite `anomaly.enabled` en YAML, `yaml_serde` puede dejarlo `None` → `unwrap_or(true)` lo trata como habilitado. Comportamiento correcto pero implícito.
- **Evidencia:** Código verificado: `AnomalyConfig::default()` → `enabled: Some(true)`. YAML parsing con `Option<bool>`: si clave ausente → `None` → `unwrap_or(true)` → enabled. Si `enabled: false` explícito → `Some(false)` → disabled.
- **Impacto:** Comportamiento razonable (enabled by default), pero no documentado explícitamente.
- **Corrección recomendada:** Documentar en CONFIGURATION.md (cuando exista) o en config.example.yaml comentario: `# enabled: true (default) — set false to disable`.

### HALLAZGO-007: Métrica `netwatch_scheduler_jobs_total` / `netwatch_scheduler_errors_total` No Instrumentadas En Scheduler
- **Severidad:** Medio
- **Documento afectado:** ARCHITECTURE_AND_FUNCTIONAL_TESTS.md (líneas 636-637: "Via AppMetrics (not yet instrumented in scheduler)"), ARCHITECTURE.md (línea 277-278: mismo), IMPLEMENTATION_STATUS.md (línea 277-278: "not yet instrumented")
- **Problema:** Las métricas `netwatch_scheduler_jobs_total` y `netwatch_scheduler_errors_total` están definidas en `AppMetrics` y registradas, **pero el `Scheduler` no las incrementa**. Documentación lo reconoce honestamente ("not yet instrumented").
- **Evidencia:** `src/scheduler.rs` no importa `AppMetrics` ni emite esas métricas. `src/metrics.rs` líneas 12-15 definen los counters. `src/main.rs` línea 705 crea `AppMetrics::new()` que los registra.
- **Impacto:** Métricas siempre en 0. No hay visibilidad de actividad/errores del scheduler via Prometheus.
- **Corrección recomendada:** Instrumentar `Scheduler::run()` y job execution para incrementar contadores. Añadir `AppMetrics` como dependencia del Scheduler o pasar callback.

### HALLAZGO-008: `discover local` CLI Documentado Como Removido En CHECKPOINT_P4 Pero Existe En Código
- **Severidad:** Bajo (error documental en checkpoint, no en docs principales)
- **Documento afectado:** CHECKPOINT_P4.md (línea 47: "**Note: `discover local` CLI subcommand was NOT removed; it remains implemented.**" — corrección en sección corrective review), línea 230: "**NEW: Removed `discover local` CLI subcommand** (simplified CLI)" — claim original incorrecto
- **Problema:** CHECKPOINT_P4.md originalmente afirmaba (línea 230) que se removió `discover local`, pero la corrección (línea 305) aclara que **NO fue removido**. El código actual (`main.rs` líneas 61-64, 493-524) **sí tiene** `DiscoverKind::Local` y handler funcional.
- **Evidencia:** `main.rs` líneas 61-64 (`DiscoverKind::Local`), 493-524 (handler imprime interfaces/routes). E2E test `test_cli_discover_local()` en `e2e_smoke.sh` línea 316-324 lo prueba.
- **Impacto:** Checkpoint tenía error factual corregido en review. Docs principales (README, DEVELOPMENT, ARCHITECTURE) lo documentan correctamente como existente.
- **Corrección recomendada:** Ya corregido en CHECKPOINT_P4.md sección "Corrective Review". No acción adicional.

### HALLAZGO-009: Falta Documentación De Variables De Entorno Para Secrets (SNMP, Telegram, Kafka)
- **Severidad:** Medio
- **Documento afectado:** config.example.yaml (líneas 57-81), DEVELOPMENT.md (líneas 235-238), README.md (línea 94-106 security table)
- **Problema:** `config.example.yaml` muestra comentarios con `${SNMP_AUTH_PASSWORD}`, `${TELEGRAM_BOT_TOKEN}`, `${GRAFANA_ADMIN_PASSWORD}` pero **no documenta** que estas variables de entorno son **requeridas** si se habilitan esas features. `Config::from_path()` (config.rs líneas 168-177) solo sobrescribe `NETWATCH_STORAGE_PATH`, `NETWATCH_LOG_LEVEL`, `NETWATCH_LISTEN` — **no lee secrets desde env** para SNMP/Telegram/Kafka.
- **Evidencia:** `config.rs` no tiene lógica para expandir `${VAR}` en YAML ni leer `SNMP_*`, `TELEGRAM_*`, `KAFKA_*` env vars. Los comentarios en config.example.yaml son aspirational.
- **Impacto:** Usuario configura `snmp.enabled: true` y pone credenciales en YAML (mal práctica) o espera que `${VAR}` se expanda (no ocurre). Secrets terminan en config file o no funcionan.
- **Corrección recomendada:**
  1. Implementar env var substitution en `Config::from_path()` para secrets, o usar `config.rs` con `figment`/`config` crate que soporte env.
  2. Documentar variables de entorno requeridas por feature en CONFIGURATION.md.
  3. En config.example.yaml, marcar claramente: `# REQUIRED if snmp.enabled: true — set via env, not in file`.

### HALLAZGO-010: Health Check Jobs En Scheduler Usan Targets Hardcoded (localhost/127.0.0.1)
- **Severidad:** Medio
- **Documento afectado:** ARCHITECTURE.md (líneas 96-100), IMPLEMENTATION_STATUS.md (línea 44: "health check jobs registered at main.rs:538, 547, 561, 575")
- **Problema:** Los 4 health check jobs (ICMP, HTTP, DNS, TLS) en `register_monitoring_jobs()` usan targets hardcoded: `"127.0.0.1"`, `"http://127.0.0.1:8080/health"`, `"localhost"`, `"localhost:8080"`. No usan inventario de dispositivos ni configuración de targets. Documentación dice "runs every `config.health.interval` seconds" pero no aclara que **solo monitoriza localhost**.
- **Evidencia:** `main.rs` líneas 103, 176, 248, 320 — targets hardcoded en closures de jobs.
- **Impacto:** Usuario espera monitoreo de red configurado; obtiene solo self-monitoring de localhost. Obj 1 "Per-device/service check scheduling" marcado como gap en ROADMAP (línea 463).
- **Corrección recomendada:** Documentar limitación actual: "Health checks run against localhost only; per-device scheduling requires inventory (Obj 5) — planned". Ya parcialmente en IMPLEMENTATION_STATUS.md "Remaining P0 Gaps".

### HALLAZGO-011: `metrics.sqlite` En Config Pero No Usado En Código
- **Severidad:** Bajo
- **Documento afectado:** config.example.yaml (líneas 11-13: `metrics.path: "/data/metrics.sqlite"`), DEVELOPMENT.md (línea 200-202)
- **Problema:** Config define `metrics.path` para base de datos separada de métricas. En código, `Storage::connect()` usa `config.storage.path` (único). `metrics.path` **nunca se lee ni usa**. `AppMetrics` usa `prometheus::default_registry()` en memoria, no SQLite.
- **Evidencia:** `config.rs` línea 18-21 define `MetricsConfig { enabled, path }`. `main.rs` línea 548 usa `config.storage.path`. `metrics.rs` no accede a config.
- **Impacto:** Configuración muerta. Usuario configura path inútil. Confusión sobre arquitectura de métricas.
- **Corrección recomendada:**
  1. Remover `metrics.path` de `Config` y `config.example.yaml` si no se usa.
  2. O implementar persistencia de métricas en SQLite (si es objetivo) y usar `metrics.path`.
  3. Documentar que métricas Prometheus son in-memory/ephemeral (salvo Prometheus externo scrapeando).

### HALLAZGO-012: `ports` Config Section En config.example.yaml Sin Uso Evidente
- **Severidad:** Bajo
- **Documento afectado:** config.example.yaml (líneas 28-30), config.rs (líneas 44-48)
- **Problema:** Config tiene sección `ports: { enabled: true, interval: 900 }`. En código, `PortsConfig` existe pero **no se usa en `main.rs`** para registrar jobs de port scanning. `main.rs` no importa `config.ports`.
- **Evidencia:** Búsqueda en `main.rs`: no hay referencia a `config.ports`. `register_monitoring_jobs()` solo registra health checks (icmp, http, dns, tls), topology, snmp, anomaly, capacity.
- **Impacto:** Configuración muerta. Usuario habilita `ports.enabled: true` esperando port scanning; no ocurre.
- **Corrección recomendada:** Remover sección `ports` de config y Config struct hasta implementar port scanning (Obj 6 / Obj 2 related), o implementar job y documentar.

### HALLAZGO-013: Inconsistencia En Nombre De Métrica `netwatch_health_check_latency_seconds` vs Documentación
- **Severidad:** Bajo
- **Documento afectado:** ARCHITECTURE.md (línea 281: `netwatch_health_check_latency_seconds`), ARCHITECTURE_AND_FUNCTIONAL_TESTS.md (línea 648: mismo), e2e_smoke.sh (líneas 468, 481: busca `netwatch_health_check_latency_seconds`)
- **Problema:** **Consistente** — el nombre en código (`src/monitoring/metrics.rs` línea 13: `HEALTH_CHECK_LATENCY_SECONDS`) coincide con docs y tests. **Sin hallazgo** — verificación pasó.
- **Nota:** Incluido para documentar verificación exitosa.

### HALLAZGO-014: Falta Documentación De Rate Limiting / Concurrency En Scheduler
- **Severidad:** Informativo
- **Documento afectado:** DEVELOPMENT.md (línea 226: `scheduler.concurrency: 10`, `scheduler.rate_limit: 1000`), config.example.yaml (líneas 36-38)
- **Problema:** Config expone `scheduler.concurrency` y `scheduler.rate_limit`. En `main.rs` línea 579: `Scheduler::new(config.scheduler.concurrency)` — **solo concurrency usado**. `rate_limit` **no se usa** en `Scheduler` ni en jobs.
- **Evidencia:** `src/scheduler.rs` líneas 1-70: `Scheduler::new(concurrency: usize)` usa semaphore para limitar jobs concurrentes. No hay rate limiting (requests/second).
- **Impacto:** Config `rate_limit` inútil. Documentación debería reflejar solo `concurrency` o implementar rate limiting.
- **Corrección recomendada:** Remover `rate_limit` de config y docs, o implementar token bucket en Scheduler.

### HALLAZGO-015: Alertmanager Config Referencia Endpoint `/api/v1/alerts/webhook` Que No Existe
- **Severidad:** Medio
- **Documento afectado:** alertmanager/alertmanager.yml (líneas 35, 40, 58, 63: `url: 'http://netwatch:8080/api/v1/alerts/webhook'`)
- **Problema:** Alertmanager config define 4 receivers apuntando a `http://netwatch:8080/api/v1/alerts/webhook`. En `src/api.rs` **no existe** ruta `/api/v1/alerts/webhook`. Router no la registra.
- **Evidencia:** `api.rs` líneas 217-240: rutas definidas. No hay `/alerts/webhook`. `main.rs` no registra handler para alerts.
- **Impacto:** Si Alertmanager dispara alerta, webhook falla 404. Stack observabilidad "Partial — stack deployment untested" (README línea 18).
- **Corrección recomendada:**
  1. Implementar endpoint `/api/v1/alerts/webhook` en `api.rs` para recibir Alertmanager webhooks.
  2. O actualizar alertmanager.yml a endpoint que exista (ej. `/api/v1/incidents/webhook` si se implementa).
  3. Documentar que Alertmanager integration está en config pero endpoint faltante.

### HALLAZGO-016: Grafana Dashboard Provisioning Referencia Archivo Que Existe Pero No Verificado
- **Severidad:** Informativo
- **Documento afectado:** compose.yaml (línea 82: `GF_DASHBOARDS_DEFAULT_HOME_DASHBOARD_PATH=/var/lib/grafana/dashboards/netwatch-overview.json`), grafana/dashboards/netwatch-overview.json (existe)
- **Problema:** Archivo `grafana/dashboards/netwatch-overview.json` existe (comprobado). Provisioning config correcto. **No verificado en runtime** (Docker no disponible). 
- **Evidencia:** Archivo existe en fs. `compose.yaml` volumen `./grafana/dashboards:/var/lib/grafana/dashboards:ro` lo monta.
- **Impacto:** Si dashboard JSON tiene errores de sintaxis o métricas inexistentes, no se detecta hasta deploy.
- **Corrección recomendada:** Validar JSON syntax y métricas referenciadas existen en netwatch (`netwatch_health_checks_total`, etc.). Añadir test CI para validar dashboard.

### HALLAZGO-017: `known-failures.txt` Existe Pero Vacío / No Documentado
- **Severidad:** Informativo
- **Documento afectado:** scripts/known-failures.txt, scripts/check.sh (línea 87, 234)
- **Problema:** `check.sh` referencia `KNOWN_FAILURES_FILE=scripts/known-failures.txt` para ignorar tests fallidos conocidos. Archivo existe pero **vacío** (0 bytes). No hay tests fallidos conocidos documentados.
- **Evidencia:** `ls -la scripts/known-failures.txt` → 0 bytes. CHECKPOINT_P4.md línea 127 menciona "TCP Test Flakiness (`cli_monitor_tcp_success`) — Status: UNVERIFIED (previously reported as intermittent)" — candidato para known-failures.
- **Impacto:** Mecanismo existe pero no usado. Si test flaky aparece, no hay proceso para documentarlo.
- **Corrección recomendada:** Documentar en DEVELOPMENT.md o CONTRIBUTING.md el uso de known-failures.txt. Añadir `cli_monitor_tcp_success` si confirmado flaky.

### HALLAZGO-018: Referencia A `config.health.interval` En main.rs Para 4 Jobs Distintos
- **Severidad:** Informativo
- **Documento afectado:** IMPLEMENTATION_STATUS.md (línea 44: "health check jobs registered at main.rs:538, 547, 561, 575 — runs every `config.health.interval` seconds")
- **Problema:** Los 4 health check jobs (ICMP, HTTP, DNS, TLS) **comparten el mismo intervalo** `config.health.interval`. No hay configuración separada por protocolo. Documentación implica granularidad que no existe.
- **Evidencia:** `main.rs` líneas 95, 167, 240, 312: todos usan `config.health.interval`.
- **Impacto:** Usuario no puede configurar intervalo distinto para ICMP vs HTTP. Limitación conocida (Obj 13 "Scheduler integration pending").
- **Corrección recomendada:** Documentar en CONFIGURATION.md: "Single `health.interval` applies to all health check protocols. Per-protocol intervals planned (Obj 13)."

### HALLAZGO-019: `AnomalyConfig` En config.example.yaml Tiene Campos No En Config Struct
- **Severidad:** Bajo
- **Documento afectado:** config.example.yaml (líneas 44-55), config.rs (líneas 69-99)
- **Problema:** Verificación: **Todos los campos en config.example.yaml existen en `AnomalyConfig` struct**. Campos: `enabled`, `interval`, `min_samples`, `lookback_window_seconds`, `sensitivity`, `min_z_score`, `enable_spike_detection`, `enable_drift_detection`, `enable_pattern_break_detection`, `anomaly_cooldown_seconds`, `min_absolute_change`. **Coinciden exactamente**. Sin hallazgo — verificación pasó.
- **Nota:** Incluido para documentar verificación exitosa.

### HALLAZGO-020: Falta Documentación De Migraciones Y Esquema DB En Docs Principales
- **Severidad:** Informativo
- **Documento afectado:** README.md, DEVELOPMENT.md, ARCHITECTURE.md
- **Problema:** ARCHITECTURE.md (líneas 243-268) documenta esquema de 14 tablas en 2 migraciones. README y DEVELOPMENT.md no mencionan migraciones ni esquema. Usuario nuevo no sabe que DB se auto-migra al iniciar.
- **Evidencia:** `storage.rs` líneas 28-31: `Migrator::new("./migrations").await` + `migrator.run(&pool).await`. Migraciones en `migrations/` (4 archivos).
- **Impacto:** Información dispersa. Desarrollador debe leer ARCHITECTURE.md para entender persistencia.
- **Corrección recomendada:** Añadir sección "Database & Migrations" en DEVELOPMENT.md o README con resumen y link a ARCHITECTURE.md.

---

## 5. Funcionalidades Sin Documentar (Implementadas En Código, Ausentes En Docs)

| Funcionalidad | Ubicación Código | Documentación Faltante |
|---------------|------------------|------------------------|
| **CLI `capacity report`** | `main.rs` líneas 54-58, 525-538 | README quick start no lo menciona; DEVELOPMENT.md no lo lista |
| **API `/incidents/hypotheses*`** | `api.rs` líneas 153-215, 230-238 | README API examples no incluye; ARCHITECTURE_AND_FUNCTIONAL_TESTS.md API reference omite |
| **IncidentEngine con root cause confirmation** | `incidents.rs` líneas 251-280 (`confirm_root_cause`), `api.rs` líneas 196-215 | IMPLEMENTATION_STATUS.md Obj 3/10 lo menciona pero sin API reference |
| **Anomaly deduplication cache** | `anomaly.rs` líneas 218-220, 546-580 | IMPLEMENTATION_STATUS.md Obj 7.14 lo lista como "NEW" pero no en README/ROADMAP |
| **Pattern break detection** | `anomaly.rs` líneas 546-680 | ROADMAP Obj 7.6 lo lista como ✅ PERO "pattern break config exposure" pendiente (7.11) |
| **Capacity `nominal_capacity` + `capacity_source`** | `capacity_planning.rs` líneas 19-25, 311-325, 394-398 | ROADMAP Obj 14.9 lista como ✅ NEW pero no en config.example.yaml docs |
| **RootCauseAnalyzer `is_root()` method** | `root_cause.rs` líneas 54-60 | CHECKPOINT_P4.md lo lista como NEW; no en ARCHITECTURE/ROADMAP principales |
| **Service nodes + DependsOn edges en topology** | `topology.rs` línea 188, `topology_builder.rs` `add_service_dependency()`, `main.rs` líneas 300-330 | ARCHITECTURE.md module map no menciona Service node type; IMPLEMENTATION_STATUS Obj 2.78 lo lista |
| **EventBus `Default` derive para EventEvidence** | `events.rs` (corrección CHECKPOINT_P4) | No documentado — cambio interno |
| **Storage `get_latency_observations_with_timestamps`** | `storage.rs` líneas 123-165 | Añadido para capacity planning; no en ARCHITECTURE.md storage section |

---

## 6. Información Obsoleta (Documentada Pero Ya No Existe / Cambió)

| Instrucción/Referencia | Documento | Estado Real | Evidencia |
|------------------------|-----------|-------------|-----------|
| "Config NOT WIRED" | ARCHITECTURE_AND_FUNCTIONAL_TESTS.md (histórico) | **FALSO** — Config wired desde P0 | CHANGES_MADE.md tabla "Discrepancies Resolved" línea 265 |
| "Storage NOT WIRED" | ARCHITECTURE_AND_FUNCTIONAL_TESTS.md (histórico) | **FALSO** — Storage wired | CHANGES_MADE.md línea 266 |
| "Scheduler NOT USED" | ARCHITECTURE_AND_FUNCTIONAL_TESTS.md (histórico) | **FALSO** — Scheduler running | CHANGES_MADE.md línea 267 |
| "EventBus NOT USED" | ARCHITECTURE_AND_FUNCTIONAL_TESTS.md (histórico) | **FALSO** — EventBus emitting | CHANGES_MADE.md línea 268 |
| "No persistence of checks" | ARCHITECTURE_AND_FUNCTIONAL_TESTS.md (histórico) | **FALSO** — persist_observation() works | CHANGES_MADE.md línea 269 |
| "P2 ✅ Complete" | ROADMAP.md (versión antigua) | **FALSO** — P2 🔄 In Progress (SNMP placeholder) | ROADMAP.md actual línea 21: "P2 🔄 In Progress" |
| "P3 Drift Detection only" | ROADMAP.md (versión antigua) | **FALSO** — P3 tiene drift ✅, maintenance 🔄, reports ⏳, DR ⏳ | ROADMAP.md actual línea 22 |
| "Incident engine NOT CONNECTED to EventBus" | ARCHITECTURE_AND_FUNCTIONAL_TESTS.md línea 390 | **FALSO** — IncidentEngine wired en main.rs:576, 586, 686-690 | main.rs líneas 576, 686-690; CHECKPOINT_P4.md línea 227-229 |
| "No incident engine consuming events" | IMPLEMENTATION_STATUS.md línea 503 (Known Limitations) | **FALSO** — IncidentEngine consume events | CHANGES_MADE.md línea 273 "Discrepancies Resolved" pero Known Limitations no actualizado |
| "No SLA/SLO calculations" | IMPLEMENTATION_STATUS.md línea 504, ARCHITECTURE.md línea 517 | **PARCIALMENTE FALSO** — `compute_percentile()` y `get_availability_stats()` implementados en storage.rs | storage.rs líneas 168-224; IMPLEMENTATION_STATUS.md Obj 1.46-47 marca ✅ |
| `discover local` CLI removed | CHECKPOINT_P4.md línea 230 (original) | **FALSO** — Command exists | Corrección en CHECKPOINT_P4.md línea 305 |

---

## 7. Validación Técnica

### Comandos / Ejemplos Verificados (Estático - Sin Ejecución)

| Comando / Ejemplo | Documento | Verificación Código | Estado |
|-------------------|-----------|---------------------|--------|
| `cargo build --release` | README, DEVELOPMENT, check.sh | `Cargo.toml` package.name = "netwatch" → binary `netwatch` | ✅ Sintaxis OK |
| `./target/release/netwatch monitor ping 1.2.3.4` | README línea 83 | `main.rs` `MonitorKind::Ping { target }` + `ping()` | ✅ Existe |
| `./target/release/netwatch monitor tcp example.com 443` | README línea 84 | `main.rs` `MonitorKind::Tcp { host, port }` + `check_tcp()` | ✅ Existe |
| `./target/release/netwatch discover local` | README línea 85, DEVELOPMENT.md | `main.rs` `DiscoverKind::Local` + `discover_interfaces/routes()` | ✅ Existe |
| `curl http://localhost:8080/health` | README línea 88 | `api.rs` `health_handler()` + router `/health` | ✅ Existe |
| `curl http://localhost:8080/api/v1/interfaces` | README línea 89 | `api.rs` `interfaces_handler()` + router | ✅ Existe |
| `curl -X POST .../monitor/ping` | README línea 90 | `api.rs` `ping_handler()` + router POST | ✅ Existe |
| `curl http://localhost:8080/metrics` | README línea 91 | `main.rs` router.merge `/metrics` + `AppMetrics::metrics_handler` | ✅ Existe |
| `netwatch capacity report --format json` | ROADMAP, IMPLEMENTATION_STATUS | `main.rs` `CapacityKind::Report { format }` | ✅ CLI existe |
| `GET /api/v1/capacity/report` | ROADMAP, IMPLEMENTATION_STATUS, P4_AUDIT | `api.rs` router — **NO EXISTE** | ❌ FALSO |
| `Config::from_path()` + env overrides | DEVELOPMENT.md, ARCHITECTURE.md | `config.rs` líneas 162-180 | ✅ Implementado |
| `cargo fmt --check` / `clippy` / `test` | DEVELOPMENT.md, check.sh | `Cargo.toml` + CI workflow | ✅ Comandos estándar |
| `cargo audit` / `cargo deny` | DEVELOPMENT.md, CHANGES_MADE.md | `.cargo/audit.toml` + `.cargo/deny.toml` (verificar existencia) | ⚠️ deny.toml no verificado |
| `./scripts/check.sh` | README, DEVELOPMENT.md, CHANGES_MADE.md | `scripts/check.sh` existe, executable en git | ✅ Existe |
| `./scripts/e2e_smoke.sh` | README, DEVELOPMENT.md, CI | `scripts/e2e_smoke.sh` existe, 42 tests definidos | ✅ Existe |

### Verificación De Enlaces Internos

| Enlace | Documento Origen | Destino Existe | Estado |
|--------|------------------|----------------|--------|
| `docs/ROADMAP.md` | README línea 32 | Sí | ✅ |
| `docs/IMPLEMENTATION_STATUS.md` | README línea 71 | Sí | ✅ |
| `docs/ARCHITECTURE.md` | README línea 112 | Sí | ✅ |
| `docs/ARCHITECTURE_AND_FUNCTIONAL_TESTS.md` | README línea 114 | Sí | ✅ |
| `docs/DEVELOPMENT.md` | README línea 115 | Sí | ✅ |
| `docs/CHANGES_MADE.md` | README línea 116 | Sí | ✅ |
| `docs/IMPLEMENTATION_STATUS.md` | ARCHITECTURE.md línea 526 | Sí | ✅ |
| `docs/ROADMAP.md` | ARCHITECTURE.md línea 527 | Sí | ✅ |
| `docs/ARCHITECTURE_AND_FUNCTIONAL_TESTS.md` | ARCHITECTURE.md línea 528 | Sí | ✅ |
| `docs/DEVELOPMENT.md` | ARCHITECTURE.md línea 528 | Sí | ✅ |
| `scripts/check.sh` | README línea 133 | Sí | ✅ |
| `scripts/e2e_smoke.sh` | DEVELOPMENT.md línea 135 | Sí | ✅ |
| `config.example.yaml` | DEVELOPMENT.md línea 190 | Sí | ✅ |
| `migrations/` | ARCHITECTURE.md línea 245 | Sí (4 archivos) | ✅ |

### Verificación De Variables De Entorno

| Variable | Documentada En | Usada En Código | Estado |
|----------|----------------|-----------------|--------|
| `NETWATCH_STORAGE_PATH` | config.example.yaml (comentario), DEVELOPMENT.md | `config.rs` línea 169 | ✅ |
| `NETWATCH_LOG_LEVEL` | config.example.yaml (comentario), DEVELOPMENT.md | `config.rs` línea 172 | ✅ |
| `NETWATCH_LISTEN` | config.example.yaml (comentario), DEVELOPMENT.md | `config.rs` línea 175 | ✅ |
| `SNMP_AUTH_PASSWORD` | config.example.yaml línea 61 | **NO** — config.rs no lee | ❌ Doc only |
| `TELEGRAM_BOT_TOKEN` | config.example.yaml línea 68, alertmanager.yml | **NO** — no implementado | ❌ Doc only |
| `GRAFANA_ADMIN_PASSWORD` | compose.yaml línea 85 | Docker env only | ⚠️ Runtime only |

### Verificación De Estructura De Directorios (README vs Real)

| Entrada README / ARCHITECTURE | Existe En FS | Estado |
|------------------------------|--------------|--------|
| `src/main.rs` | Sí | ✅ |
| `src/lib.rs` | Sí | ✅ |
| `src/config.rs` | Sí | ✅ |
| `src/logging.rs` | Sí | ✅ |
| `src/api.rs` | Sí | ✅ |
| `src/metrics.rs` | Sí | ✅ |
| `src/storage.rs` | Sí | ✅ |
| `src/scheduler.rs` | Sí | ✅ |
| `src/events.rs` | Sí | ✅ |
| `src/incidents.rs` | Sí | ✅ |
| `src/drift.rs` | Sí | ✅ |
| `src/topology.rs` | Sí | ✅ |
| `src/topology_builder.rs` | Sí | ✅ |
| `src/topology_store.rs` | Sí | ✅ |
| `src/snmp.rs` | Sí | ✅ |
| `src/monitoring/` (6 archivos) | Sí | ✅ |
| `src/discovery/` (3 archivos) | Sí | ✅ |
| `migrations/` (4 archivos) | Sí | ✅ |
| `tests/` (10 archivos) | Sí | ✅ |

### Pruebas No Ejecutadas (Pendientes De Verificación)

| Prueba | Comando | Bloqueador | Acción Requerida |
|--------|---------|------------|------------------|
| Unit/Integration tests | `cargo test` | `dlltool.exe` missing | Instalar MinGW o `rustup target add x86_64-pc-windows-msvc` |
| Clippy | `cargo clippy --all-targets --all-features -- -D warnings` | `dlltool.exe` missing | Same |
| Release build | `cargo build --release` | `dlltool.exe` missing | Same |
| E2E smoke test | `./scripts/e2e_smoke.sh` | Requiere release binary | Same |
| Docker build | `docker build` | Docker Desktop no disponible en WSL | Instalar Docker Desktop + WSL integration |
| Cargo audit | `cargo audit` | `dlltool.exe` missing | Same |
| Cargo deny | `cargo deny check...` | `dlltool.exe` missing / falta `.cargo/deny.toml` | Verificar deny.toml existe |

---

## 8. Plan De Actualización

### Prioridad 1 — Correcciones Críticas/De Seguridad (Antes De Cualquier Release)

| # | Acción | Documento(s) | Esfuerzo |
|---|--------|--------------|----------|
| 1 | Resolver toolchain issue y re-ejecutar `./scripts/check.sh` completo | README, DEVELOPMENT, IMPLEMENTATION_STATUS, CHECKPOINT_P4 | Alto (entorno) |
| 2 | Actualizar Known Limitations en IMPLEMENTATION_STATUS.md: remover "No incident engine consuming events" (ya wired) | IMPLEMENTATION_STATUS.md línea 503 | Bajo |
| 3 | Corregir referencia línea `main.rs:535` → buscar por `Scheduler::new()` | IMPLEMENTATION_STATUS.md, ARCHITECTURE.md | Bajo |
| 4 | Aclarar/remover config SNMP placeholder en config.example.yaml y DEVELOPMENT.md | config.example.yaml, DEVELOPMENT.md | Bajo |

### Prioridad 2 — Completitud De API Y Documentación De Endpoints

| # | Acción | Documento(s) | Esfuerzo |
|---|--------|--------------|----------|
| 5 | Implementar endpoint `/api/v1/capacity/report` en `api.rs` O actualizar docs a "CLI only" | api.rs, ROADMAP, IMPLEMENTATION_STATUS, P4_AUDIT | Medio |
| 6 | Implementar endpoint `/api/v1/alerts/webhook` para Alertmanager integration | api.rs, alertmanager.yml | Medio |
| 7 | Añadir referencia a CLI `capacity report` y API `/incidents/hypotheses*` en README quick start | README.md | Bajo |
| 8 | Documentar variables de entorno requeridas para secrets (SNMP, Telegram, Kafka) | CONFIGURATION.md (crear), config.example.yaml | Medio |

### Prioridad 3 — Limpieza De Configuración Muerta

| # | Acción | Documento(s) | Esfuerzo |
|---|--------|--------------|----------|
| 9 | Remover `metrics.path` de Config y config.example.yaml (no usado) | config.rs, config.example.yaml, DEVELOPMENT.md | Bajo |
| 10 | Remover `ports` section de Config y config.example.yaml (no usado) | config.rs, config.example.yaml, DEVELOPMENT.md | Bajo |
| 11 | Remover `scheduler.rate_limit` de Config (no usado — solo `concurrency`) | config.rs, config.example.yaml, DEVELOPMENT.md | Bajo |
| 12 | Documentar que `health.interval` aplica a todos los protocolos (no per-protocol) | CONFIGURATION.md, IMPLEMENTATION_STATUS.md | Bajo |

### Prioridad 4 — Documentos Faltantes Planificados (ROADMAP Obj 8)

| # | Documento | Contenido Esperado | Esfuerzo |
|---|-----------|-------------------|----------|
| 13 | CONFIGURATION.md | Referencia completa config.yaml, env vars, precedencia, secrets | Medio |
| 14 | API.md | OpenAPI spec o referencia completa endpoints, request/response, errores | Medio |
| 15 | OPERATIONS.md | Deploy, backup/restore, monitoring, troubleshooting, scaling | Alto |
| 16 | CONTRIBUTING.md | Guidelines, PR process, coding standards, test requirements | Medio |
| 17 | SECURITY.md | Threat model, vulnerability reporting, hardening, audit process | Medio |

### Prioridad 5 — Mejoras De Calidad

| # | Acción | Documento(s) | Esfuerzo |
|---|--------|--------------|----------|
| 18 | Homogeneizar contadores de tests: explicar base de "91" vs "46+42" | DEVELOPMENT.md, IMPLEMENTATION_STATUS.md | Bajo |
| 19 | Añadir nota "Database & Migrations" en DEVELOPMENT.md con link a ARCHITECTURE.md | DEVELOPMENT.md | Bajo |
| 20 | Validar `grafana/dashboards/netwatch-overview.json` métricas vs código | grafana dashboard, CI | Medio |
| 21 | Documentar uso de `scripts/known-failures.txt` en DEVELOPMENT/CONTRIBUTING | DEVELOPMENT.md | Bajo |
| 22 | Instrumentar `Scheduler` con `netwatch_scheduler_jobs_total` / `errors_total` | scheduler.rs, metrics.rs | Medio |

---

## 9. Conclusión

### Clasificación Global De La Documentación

**Mayoritariamente correcta, con ajustes pendientes**

### Justificación

**Evidencias de corrección:**
- ✅ 10 de 11 documentos principales reflejan con precisión el estado actual de la implementación (status Partial/Implemented/Planned correctos).
- ✅ Distinción honesta entre implementado vs planificado en tablas de objetivos (README, IMPLEMENTATION_STATUS, ROADMAP).
- ✅ Referencias cruzadas entre documentos funcionan (links internos válidos).
- ✅ Arquitectura documentada (ARCHITECTURE.md, ARCHITECTURE_AND_FUNCTIONAL_TESTS.md) coincide con código real: module map, component responsibilities, execution flows, data flow diagram.
- ✅ CHANGES_MADE.md documenta transparente la reconciliación docs-código histórica.
- ✅ Checkpoints y auditorías (P3, P4) proporcionan trazabilidad de decisiones y estado conocido.

**Ajustes pendientes (no invalidan la documentación general):**
1. **Tests no re-verificados** (Alto) — etiquetados como "historical" pero usuarios pueden pasar por alto.
2. **SNMP config placeholder** (Alto) — config sugiere funcionalidad inexistente.
3. **Endpoint `/api/v1/capacity/report` documentado pero no implementado** (Medio).
4. **Alertmanager webhook endpoint faltante** (Medio).
5. **Configuración muerta** (`metrics.path`, `ports`, `scheduler.rate_limit`) (Bajo).
6. **Contadores de tests inconsistentes** (Bajo) — por categorización, no error.
7. **5 documentos planificados ausentes** (Informativo) — ya rastreados en ROADMAP Obj 8.

**No verificable con evidencia disponible:**
- Estado real de tests (bloqueado por toolchain).
- Despliegue Docker/Compose stack (sin Docker).
- Comportamiento runtime de endpoints bajo carga.

### Recomendación Final

La documentación **puede considerarse confiable para desarrollo y operación** con las salvedades documentadas. Los hallazgos de prioridad 1-2 deben resolverse antes de considerar la documentación "actualizada y coherente" sin reservas. La base documental es sólida y el proceso de reconciliación (evidenciado en CHANGES_MADE.md y checkpoints) demuestra madurez en mantenimiento docs-código.

---

## Apéndice: Metodología De Auditoría

Esta auditoría siguió las 9 fases especificadas:

1. **Fase 1 - Reconocimiento:** Inspección fs, git status, log, estructura, configs.
2. **Fase 2 - Historial Git:** Análisis 30 commits, diffs de commits documentación (14a4b6d, e50c5b1, 057a481, ae2b030, f17dfbf, 183165d, 27dcdb8), verificación sincronización remoto.
3. **Fase 3 - Inventario Documental:** 11 archivos .md localizados, propósito/audiencia/cobertura mapeados.
4. **Fase 4 - Auditoría Código-Orientada-Docs:** Lectura de 22 archivos fuente principales, configs, migraciones, tests; contraste claims vs implementación.
5. **Fase 5 - Validación Comandos/Ejemplos:** Verificación estática de comandos README/DEVELOPMENT vs Cargo.toml, main.rs, api.rs, scripts.
6. **Fase 6 - Coherencia Temporal:** Comparación docs actuales vs commits históricos (CHANGES_MADE.md tabla discrepancies, CHECKPOINT_P4 corrective review).
7. **Fase 7 - Clasificación Hallazgos:** 20 hallazgos clasificados Crítico/Alto/Medio/Bajo/Informativo con justificación.
8. **Fase 8 - Informe:** Este documento.
9. **Fase 9 - Actualización:** **Pendiente autorización** — no se modificaron archivos durante auditoría.

**Herramientas usadas:** Lectura de archivos, grep/glob para símbolos, git log/show/diff, análisis estático Rust (sin compilación).
