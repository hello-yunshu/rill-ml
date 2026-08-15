/*
 * Forwarding header for the RillML Stable C ABI.
 *
 * SwiftPM requires a C target's public headers to live inside the target's
 * `include/` directory, so this header re-exports the single source of truth
 * at crates/rill-ml-ffi/include/rill_ml.h (relative include; clang resolves
 * quoted includes from the including file's location).
 */
#ifndef RILL_ML_IOS_FORWARD_H
#define RILL_ML_IOS_FORWARD_H

#include "../../../../include/rill_ml.h"

#endif /* RILL_ML_IOS_FORWARD_H */
