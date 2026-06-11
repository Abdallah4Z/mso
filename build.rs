fn main() {
    #[cfg(not(target_os = "linux"))]
    {
        compile_error!("MSO currently only supports Linux (requires /proc for telemetry and process monitoring)");
    }
}
