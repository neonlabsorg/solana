

pub fn compute_meter_remaining() {
    #[cfg(target_arch = "bpf")]
    {
        extern "C" {
            fn sol_compute_meter_remaining() -> u64;
        }

        unsafe {
            sol_compute_meter_remaining();
        }
    }
}

// pub fn compute_meter_remaining(remaining: &mut u64) {
//     #[cfg(target_arch = "bpf")]
//     {
//         extern "C" {
//             fn sol_compute_meter_remaining(remaining: *mut u8) -> u64;
//         }
//
//         unsafe {
//             sol_compute_meter_remaining(remaining as *mut _ as *mut u8);
//         }
//     }
// }
