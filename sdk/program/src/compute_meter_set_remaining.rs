

pub fn compute_meter_set_remaining() {
    #[cfg(target_arch = "bpf")]
    {
        extern "C" {
            fn sol_compute_meter_set_remaining() -> u64;
        }

        unsafe {
            sol_compute_meter_set_remaining();
        }
    }
}


// pub fn compute_meter_set_remaining(remaining: &u64) {
//     #[cfg(target_arch = "bpf")]
//     {
//         extern "C" {
//             fn sol_compute_meter_set_remaining(remaining: &u64) -> u64;
//         }
//
//         unsafe {
//             sol_compute_meter_set_remaining(remaining);
//         }
//     }
// }
