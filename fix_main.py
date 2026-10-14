with open('/home/depdios/projects/netwatch/src/main.rs', 'r') as f:
    content = f.read()

# Add CapacityKind enum after MonitorKind enum
content = content.replace(
    '''#[derive(Subcommand)]
enum MonitorKind {
    Ping { target: String },
    Tcp { host: String, port: u16 },
}''',
    '''#[derive(Subcommand)]
enum MonitorKind {
    Ping { target: String },
    Tcp { host: String, port: u16 },
}

#[derive(Subcommand)]
enum CapacityKind {
    /// Generate capacity planning report
    Report {
        #[arg(long, default_value = 
