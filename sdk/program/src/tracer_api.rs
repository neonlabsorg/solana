

pub fn send_trace_message(val: *const u8) {
    #[cfg(target_arch = "bpf")]
        {
            extern "C" {
                fn sol_send_trace_message(val: *const u8) -> u64;
            }

            unsafe {
                sol_send_trace_message(val);
            }
        }
}
