//! Facts about this computer.

/// Total physical memory in MB, or 0 if it can't be read.
pub fn total_memory_mb() -> u64 {
    match sysinfo::System::new() {
        Ok(mut sys) => {
            sys.refresh_memory();
            sys.total_memory() / (1024 * 1024)
        }
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_memory() {
        assert!(super::total_memory_mb() > 0);
    }
}
