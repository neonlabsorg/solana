#pragma once
/**
 * @brief big_mod_exp system call
 */

#include <sol/types.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @param input ...
 * @param input_size ...
 * @param result 64 byte array to hold the result. ...
 * @return 0 if executed successfully
 */
uint64_t sol_big_mod_exp(
        const uint8_t *input,
        const unit64_t input_size,
        uint8_t *result
);


#ifdef __cplusplus
}
#endif

/**@}*/
