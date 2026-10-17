/* Test-only body appended after DataLayout offsets and independent goldens.
 * No Map/String address survives a synchronous checkpoint. Corrupt allocation
 * extents cannot be established from a header: faults remain runner failures.
 */
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

_Static_assert(POINTER_BITS == 64, "ordered-map observer requires 64-bit target");
_Static_assert(sizeof(void *) * CHAR_BIT == POINTER_BITS, "pointer width mismatch");
_Static_assert(sizeof(uintptr_t) * CHAR_BIT == POINTER_BITS, "uintptr width mismatch");
_Static_assert(sizeof(size_t) * CHAR_BIT == POINTER_BITS, "size_t width mismatch");
_Static_assert(sizeof(uint64_t) * CHAR_BIT == POINTER_BITS, "index width mismatch");
_Static_assert(KEY_STRING == 0 || KEY_STRING == 1, "invalid key kind");
_Static_assert(S_STRIDE > 0, "zero slot stride");
_Static_assert(S_STATE >= 0 && S_STATE + sizeof(uint32_t) <= S_STRIDE, "state outside slot");
_Static_assert(S_VALUE >= 0 && S_VALUE + sizeof(int32_t) <= S_STRIDE, "value outside slot");
_Static_assert(S_PREV >= 0 && S_PREV + sizeof(uint64_t) <= S_STRIDE, "prev outside slot");
_Static_assert(S_NEXT >= 0 && S_NEXT + sizeof(uint64_t) <= S_STRIDE, "next outside slot");
_Static_assert(S_KEY >= 0 && (KEY_STRING || S_KEY + sizeof(int32_t) <= S_STRIDE),
               "integer key outside slot");
_Static_assert(!KEY_STRING || (STRING_DATA >= 0 &&
               S_KEY + STRING_DATA + sizeof(void *) <= S_STRIDE), "string data outside slot");
_Static_assert(!KEY_STRING || (STRING_LENGTH >= 0 &&
               S_KEY + STRING_LENGTH + sizeof(uint64_t) <= S_STRIDE), "string length outside slot");

enum { OM_LIMIT = 128, OM_EMPTY = 0, OM_OCCUPIED = 1, OM_DELETED = 2 };
static const uint64_t OM_NONE = UINT64_MAX;
static size_t om_observed = 0;
static int om_finished = 0;

static _Noreturn void om_fail(const char *category, int64_t step) {
    fprintf(stderr, "ordered-map failure %s step %lld\n", category, (long long)step);
    exit(86);
}

static void om_require(int condition, const char *category, int64_t step) {
    if (!condition) {
        om_fail(category, step);
    }
}

static uint64_t om_read_index(const unsigned char *address) {
    uint64_t value;
    memcpy(&value, address, sizeof(value));
    return value;
}

static uint32_t om_read_state(const unsigned char *address) {
    uint32_t value;
    memcpy(&value, address, sizeof(value));
    return value;
}

static int64_t om_read_int(const unsigned char *address) {
    int32_t value;
    memcpy(&value, address, sizeof(value));
    return value;
}

static const unsigned char *om_read_pointer(const unsigned char *address) {
    const unsigned char *value;
    memcpy(&value, address, sizeof(value));
    return value;
}

static void om_check_range(const unsigned char *pointer, size_t bytes,
                           const char *category, int64_t step) {
    if (bytes != 0) {
        om_require(pointer != NULL, category, step);
        om_require((uintptr_t)pointer <= UINTPTR_MAX - (bytes - 1), category, step);
    }
}

static const unsigned char *om_slot(const unsigned char *buffer, uint64_t capacity,
                                     uint64_t index, int64_t step) {
    /* Check NONE and the capacity before even forming the slot address. */
    om_require(index != OM_NONE && index < capacity, "slot-index", step);
    return buffer + (size_t)index * (size_t)S_STRIDE;
}

static const unsigned char *om_occupied_slot(const unsigned char *buffer,
                                              uint64_t capacity, uint64_t index,
                                              const char *category, int64_t step) {
    const unsigned char *slot = om_slot(buffer, capacity, index, step);
    om_require(om_read_state(slot + S_STATE) == OM_OCCUPIED, category, step);
    return slot;
}

/* The caller has checked the slot's current Occupied state. */
static int om_key_equal(const unsigned char *slot, const ExpectedEntry *expected,
                        int64_t step) {
    if (!KEY_STRING) {
        return om_read_int(slot + S_KEY) == expected->integer;
    }
    const uint64_t length = om_read_index(slot + S_KEY + STRING_LENGTH);
    om_require(length <= OM_LIMIT, "string-length-bound", step);
    if (length != expected->length) {
        return 0;
    }
    if (length == 0) {
        /* Neither the live nor the golden byte pointer is read for an empty key. */
        return 1;
    }
    const unsigned char *bytes = om_read_pointer(slot + S_KEY + STRING_DATA);
    om_check_range(bytes, (size_t)length, "string-byte-range", step);
    om_require(expected->bytes != NULL, "golden-string-pointer", step);
    return memcmp(bytes, expected->bytes, (size_t)length) == 0;
}

static void om_check_payload(const unsigned char *slot, uint64_t index,
                              const ExpectedEntry *expected, int64_t step,
                              const char *key_category, const char *value_category) {
    om_require(om_key_equal(slot, expected, step), key_category, step);
    om_require(om_read_int(slot + S_VALUE) == expected->value, value_category, step);
    if (expected->slot >= 0) {
        om_require(index == (uint64_t)expected->slot, "golden-physical-slot", step);
    }
}

void koven_ordered_map_finish(void);

void koven_ordered_map_observe(const void *header, int32_t step) {
    om_require(!om_finished, "snapshot-after-finish", step);
    om_require(om_observed < SNAPSHOT_COUNT, "snapshot-extra", step);
    const Snapshot *expected = &EXPECTED[om_observed];
    om_require(step >= 0 && (uint64_t)step == expected->step, "snapshot-sequence", step);
    if (om_observed != 0) {
        om_require(EXPECTED[om_observed - 1].step < expected->step,
                   "golden-step-order", step);
    }
    om_require(expected->capacity <= OM_LIMIT && expected->size <= expected->capacity &&
               expected->tombstones <= expected->capacity &&
               expected->size + expected->tombstones <= expected->capacity,
               "golden-bounds", step);
    om_require(expected->size == 0 || expected->entries != NULL, "golden-entries", step);
    for (size_t i = 0; i < expected->size; ++i) {
        om_require(!KEY_STRING || expected->entries[i].length <= OM_LIMIT,
                   "golden-string-length", step);
        om_require(expected->entries[i].slot < 0 ||
                   (uint64_t)expected->entries[i].slot < expected->capacity,
                   "golden-slot-bound", step);
    }

    om_require(header != NULL, "header-null", step);
    const unsigned char *bytes = (const unsigned char *)header;
    const uint64_t capacity = om_read_index(bytes + H_CAPACITY);
    const uint64_t size = om_read_index(bytes + H_SIZE);
    const uint64_t tombstones = om_read_index(bytes + H_TOMBSTONES);
    om_require(capacity <= OM_LIMIT, "capacity-bound", step);
    om_require(size <= capacity && tombstones <= capacity && size + tombstones <= capacity,
               "header-count-bounds", step);
    om_require(capacity == expected->capacity, "capacity-golden", step);
    om_require(size == expected->size, "size-golden", step);
    om_require(tombstones == expected->tombstones, "tombstones-golden", step);
    om_require(capacity <= SIZE_MAX / (size_t)S_STRIDE &&
               capacity <= (uint64_t)PTRDIFF_MAX / (size_t)S_STRIDE,
               "buffer-size-overflow", step);
    const unsigned char *buffer = om_read_pointer(bytes + H_BUFFER);
    om_check_range(buffer, (size_t)capacity * (size_t)S_STRIDE, "buffer-range", step);
    const uint64_t first = om_read_index(bytes + H_FIRST);
    const uint64_t last = om_read_index(bytes + H_LAST);

    unsigned char buckets[OM_LIMIT] = {0};
    unsigned char forward[OM_LIMIT] = {0};
    unsigned char reverse[OM_LIMIT] = {0};
    unsigned char golden_seen[OM_LIMIT] = {0};
    size_t occupied_count = 0;
    size_t deleted_count = 0;

    /* B is an independent scan of all physical buckets, never a chain walk. */
    for (uint64_t index = 0; index < capacity; ++index) {
        const unsigned char *slot = om_slot(buffer, capacity, index, step);
        const uint32_t state = om_read_state(slot + S_STATE);
        om_require(state == OM_EMPTY || state == OM_OCCUPIED || state == OM_DELETED,
                   "bucket-state", step);
        if (state == OM_DELETED) {
            ++deleted_count;
        }
        if (state != OM_OCCUPIED) {
            continue; /* Empty/Deleted payload and links may contain stale bytes. */
        }
        buckets[index] = 1;
        ++occupied_count;
        size_t match = 0;
        size_t matches = 0;
        for (size_t i = 0; i < expected->size; ++i) {
            if (om_key_equal(slot, &expected->entries[i], step)) {
                match = i;
                ++matches;
            }
        }
        om_require(matches == 1, "bucket-key-golden", step);
        om_require(!golden_seen[match], "bucket-duplicate-key", step);
        golden_seen[match] = 1;
        om_check_payload(slot, index, &expected->entries[match], step,
                         "bucket-key-golden", "bucket-value-golden");
    }
    om_require(occupied_count == size, "occupied-count", step);
    om_require(deleted_count == tombstones, "deleted-count", step);
    for (size_t i = 0; i < expected->size; ++i) {
        om_require(golden_seen[i], "golden-key-missing", step);
    }

    if (size == 0) {
        om_require(first == OM_NONE && last == OM_NONE, "empty-endpoints", step);
    } else {
        const unsigned char *head = om_occupied_slot(buffer, capacity, first,
                                                     "first-not-occupied", step);
        const unsigned char *tail = om_occupied_slot(buffer, capacity, last,
                                                     "last-not-occupied", step);
        om_require(om_read_index(head + S_PREV) == OM_NONE, "first-prev", step);
        om_require(om_read_index(tail + S_NEXT) == OM_NONE, "last-next", step);
    }

    uint64_t current = first;
    uint64_t previous = OM_NONE;
    for (size_t position = 0; position < (size_t)size; ++position) {
        const unsigned char *slot = om_occupied_slot(buffer, capacity, current,
                                                     "forward-not-occupied", step);
        om_require(!forward[current], "forward-repeat", step);
        forward[current] = 1;
        const uint64_t prev = om_read_index(slot + S_PREV);
        const uint64_t next = om_read_index(slot + S_NEXT);
        om_require(prev == previous, "forward-prev", step);
        if (prev != OM_NONE) {
            const unsigned char *neighbor = om_occupied_slot(buffer, capacity, prev,
                                                              "forward-prev-state", step);
            om_require(om_read_index(neighbor + S_NEXT) == current, "forward-prev-next", step);
        }
        if (next != OM_NONE) {
            const unsigned char *neighbor = om_occupied_slot(buffer, capacity, next,
                                                              "forward-next-state", step);
            om_require(om_read_index(neighbor + S_PREV) == current, "forward-next-prev", step);
        }
        om_check_payload(slot, current, &expected->entries[position], step,
                         "forward-key-order", "forward-value-order");
        if (position + 1 == (size_t)size) {
            om_require(current == last && next == OM_NONE, "forward-tail", step);
        } else {
            om_require(next != OM_NONE, "forward-short", step);
        }
        previous = current;
        current = next;
    }
    om_require(current == OM_NONE, "forward-end", step);

    /* R starts afresh at last and follows the actual prev fields. */
    current = last;
    uint64_t successor = OM_NONE;
    for (size_t position = 0; position < (size_t)size; ++position) {
        const unsigned char *slot = om_occupied_slot(buffer, capacity, current,
                                                     "reverse-not-occupied", step);
        om_require(!reverse[current], "reverse-repeat", step);
        reverse[current] = 1;
        const uint64_t prev = om_read_index(slot + S_PREV);
        const uint64_t next = om_read_index(slot + S_NEXT);
        om_require(next == successor, "reverse-next", step);
        if (next != OM_NONE) {
            const unsigned char *neighbor = om_occupied_slot(buffer, capacity, next,
                                                              "reverse-next-state", step);
            om_require(om_read_index(neighbor + S_PREV) == current, "reverse-next-prev", step);
        }
        if (prev != OM_NONE) {
            const unsigned char *neighbor = om_occupied_slot(buffer, capacity, prev,
                                                              "reverse-prev-state", step);
            om_require(om_read_index(neighbor + S_NEXT) == current, "reverse-prev-next", step);
        }
        om_check_payload(slot, current, &expected->entries[(size_t)size - 1 - position], step,
                         "reverse-key-order", "reverse-value-order");
        if (position + 1 == (size_t)size) {
            om_require(current == first && prev == OM_NONE, "reverse-head", step);
        } else {
            om_require(prev != OM_NONE, "reverse-short", step);
        }
        successor = current;
        current = prev;
    }
    om_require(current == OM_NONE, "reverse-end", step);
    for (uint64_t index = 0; index < capacity; ++index) {
        om_require(buckets[index] == forward[index], "bucket-forward-set", step);
        om_require(buckets[index] == reverse[index], "bucket-reverse-set", step);
        om_require(forward[index] == reverse[index], "forward-reverse-set", step);
    }
    ++om_observed;
    fprintf(stderr, "snapshot %lld ok\n", (long long)step);
    if (om_observed == SNAPSHOT_COUNT) {
        koven_ordered_map_finish();
    }
}

void koven_ordered_map_finish(void) {
    const int64_t step = om_observed == 0 ? -1 : (int64_t)EXPECTED[om_observed - 1].step;
    om_require(!om_finished, "finish-duplicate", step);
    om_require(SNAPSHOT_COUNT != 0, "finish-no-goldens", step);
    om_require(om_observed == SNAPSHOT_COUNT, "finish-missing-snapshot", step);
    om_finished = 1;
    fprintf(stderr, "complete %zu\n", om_observed);
}
