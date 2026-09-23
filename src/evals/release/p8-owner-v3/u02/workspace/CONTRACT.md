# Reservation contract

All example inputs are nonnegative safe integer counts/timestamps. canReserve(used, capacity, requested) accepts exactly when used + requested is at most capacity. The caller ensures that sum is a safe integer. isActive(now, expiresAt) is true exactly before expiresAt; equality is expired. receipt(id) converts the supplied nonnegative integer identifier to its decimal string. No behavior outside this input contract is requested.
