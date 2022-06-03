

pub fn send_trace_message(val: &[u8]) {
    #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_send_trace_message(val: *const u8, val_len: u64) -> u64;
            }

            unsafe {
                sol_send_trace_message(
                    val as *const _ as *const u8,
                    val.len() as u64,
                );
            }
        }
}
