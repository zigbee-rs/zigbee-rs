# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "zigpy==1.6.0",
# ]
# ///
"""
Reference implementation of the ZCL wire format using zigpy.

Generates byte-exact fixtures that cross-validate zigbee-cluster-library's
encode/decode against an independent implementation. The Rust tests and this
script form a contract: if they agree, the wire encoding is correct; if they
diverge, one of the two has a bug.

Regenerate vectors when protocol handling changes:
    uv run scripts/gen_vectors.py --out zigbee-cluster-library/tests/vectors/

Verify committed fixtures match current output:
    uv run scripts/gen_vectors.py --out /tmp/check --check
"""

import argparse
import difflib
import importlib.metadata
import json
import struct
import sys
from collections.abc import Callable, Iterable, Mapping, Sequence
from pathlib import Path
from typing import TypeAlias

import zigpy.types as t
import zigpy.zcl.foundation as f

ZIGPY_VERSION = importlib.metadata.version("zigpy")
OUTPUT_FILE = "zcl_data_types.json"
SOURCE = str(Path(*Path(__file__).parts[-2:]))

JsonValue: TypeAlias = (
    None | bool | int | float | str | Sequence["JsonValue"] | Mapping[str, "JsonValue"]
)
JsonObject: TypeAlias = dict[str, JsonValue]
Serializer: TypeAlias = Callable[[object], bytes]
NamedValue: TypeAlias = tuple[str, JsonValue]
Generator: TypeAlias = tuple[int, Callable[[], JsonObject]]


def zcl_type_id(data_type: f.DataTypeId) -> t.uint8_t:
    return t.uint8_t(int(data_type))


TYPE_BOOL = zcl_type_id(f.DataTypeId.bool_)
TYPE_U8 = zcl_type_id(f.DataTypeId.uint8)
TYPE_U16 = zcl_type_id(f.DataTypeId.uint16)
TYPE_I16 = zcl_type_id(f.DataTypeId.int16)
TYPE_ARRAY = zcl_type_id(f.DataTypeId.array)
TYPE_STRUCT = zcl_type_id(f.DataTypeId.struct)
TYPE_SET = zcl_type_id(f.DataTypeId.set)
TYPE_BAG = zcl_type_id(f.DataTypeId.bag)


def hex_wire(wire: bytes) -> str:
    return " ".join(f"{byte:02X}" for byte in wire)


def named_wire(name: str, wire: bytes, **fields: JsonValue) -> JsonObject:
    return {"name": name, "wire": hex_wire(wire), **fields}


def type_mismatch_case(name: str, wire: bytes, *, expected: int, found: int) -> JsonObject:
    return named_wire(name, wire, expected=expected, found=found)


def invalid_case(name: str, wire: bytes, error: str, *, expected: int | None = None, found: int | None = None) -> JsonObject:
    extras = {k: v for k, v in {"expected": expected, "found": found}.items() if v is not None}
    return named_wire(name, wire, error=error, **extras)


def value_case(name: str, wire: bytes, value: JsonValue) -> JsonObject:
    return named_wire(name, wire, value=value)


def values_case(name: str, wire: bytes, values: Sequence[JsonValue]) -> JsonObject:
    return named_wire(name, wire, values=list(values))


def bits_case(name: str, wire: bytes, bits: int) -> JsonObject:
    return named_wire(name, wire, bits=bits)


def bytes_case(name: str, wire: bytes, value: bytes) -> JsonObject:
    return named_wire(name, wire, value_hex=hex_wire(value))


def fixture(**sections: list[JsonObject]) -> JsonObject:
    return {key: value for key, value in sections.items() if value}


def zigpy_serializer(zigpy_type: type) -> Serializer:
    return lambda value: zigpy_type(value).serialize()


def roundtrip_values(
    samples: Iterable[NamedValue], serialize: Serializer
) -> list[JsonObject]:
    return [value_case(name, serialize(value), value) for name, value in samples]


def gen_int(zigpy_type: type, bits: int, *, signed: bool) -> JsonObject:
    if signed:
        null_value = -(1 << (bits - 1))
        samples = [
            ("zero", 0),
            ("one", 1),
            ("minus_one", -1),
            ("max", (1 << (bits - 1)) - 1),
            ("min_non_null", null_value + 1),
        ]
    else:
        null_value = (1 << bits) - 1
        max_valid = null_value - 1
        samples = [
            ("zero", 0),
            ("one", 1),
            ("midpoint", max_valid // 2 + 1),
            ("max_non_null", max_valid),
        ]

    serialize = zigpy_serializer(zigpy_type)
    return fixture(
        roundtrip=roundtrip_values(samples, serialize),
        null_wire=[named_wire("null", serialize(null_value))],
    )


def gen_bool() -> JsonObject:
    serialize = zigpy_serializer(t.Bool)
    return fixture(
        roundtrip=roundtrip_values([("false", False), ("true", True)], serialize),
        null_wire=[named_wire("null", bytes([0xFF]))],
        invalid_value=[
            named_wire("reserved_low", bytes([0x02])),
            named_wire("reserved_high", bytes([0xFE])),
        ],
    )


def float_bits(value: float, *, width: int) -> int:
    if width == 32:
        return struct.unpack("<I", struct.pack("<f", value))[0]
    if width == 64:
        return struct.unpack("<Q", struct.pack("<d", value))[0]
    raise ValueError(f"unsupported float width: {width}")


def gen_float(zigpy_type: type, width: int) -> JsonObject:
    serialize = zigpy_serializer(zigpy_type)
    samples = [
        ("zero", 0.0),
        ("one", 1.0),
        ("minus_one", -1.0),
        ("one_point_five", 1.5),
    ]
    return fixture(
        roundtrip_bits=[
            bits_case(name, serialize(value), float_bits(value, width=width))
            for name, value in samples
        ],
        null_wire=[named_wire("canonical_nan_null", serialize(float("nan")))],
    )


def gen_bitmap(zigpy_type: type, bits: int) -> JsonObject:
    all_ones = (1 << bits) - 1
    samples = [
        ("zero", 0),
        ("one", 1),
        ("alternating_low", 0b10101010),
        ("all_ones", all_ones),
    ]
    serialize = zigpy_serializer(zigpy_type)
    return fixture(
        roundtrip_raw=roundtrip_values(
            ((n, v) for n, v in samples if v <= all_ones), serialize
        )
    )


def gen_enum(
    zigpy_type: type, samples: Sequence[NamedValue], null_value: int
) -> JsonObject:
    serialize = zigpy_serializer(zigpy_type)
    return fixture(
        roundtrip=roundtrip_values(samples, serialize),
        null_wire=[named_wire("null", serialize(null_value))],
    )


def counted_bytes(value: object) -> bytes:
    assert isinstance(value, (str, bytes)), f"expected str or bytes, got {type(value).__name__}"
    raw = value.encode() if isinstance(value, str) else value
    return bytes([len(raw)]) + raw


def gen_short_text() -> JsonObject:
    samples = [("empty", ""), ("ascii", "hello"), ("unicode", "café")]
    return fixture(
        roundtrip=roundtrip_values(samples, counted_bytes),
        null_wire=[named_wire("null", bytes([0xFF]))],
        invalid_utf8=[named_wire("invalid_utf8", bytes([0x03, 0xFF, 0xFE, 0xFD]))],
    )


def gen_short_octet_string() -> JsonObject:
    samples = [
        ("empty", b""),
        ("single_zero", b"\x00"),
        ("binary", b"\xde\xad\xbe\xef"),
    ]
    return fixture(
        roundtrip=[
            bytes_case(name, counted_bytes(value), value) for name, value in samples
        ],
        null_wire=[named_wire("null", bytes([0xFF]))],
    )


# Collection generators (Array, Bag, Set, Struct)
def zigpy_list(zigpy_type: type, values: Sequence[JsonValue]) -> t.LVList:
    return t.LVList[zigpy_type, t.uint16_t]([zigpy_type(value) for value in values])


def zigpy_collection(
    collection_type: type[f.TypedCollection],
    elem_type_id: int,
    zigpy_type: type,
    values: Sequence[JsonValue],
) -> bytes:
    return collection_type(
        type=t.uint8_t(elem_type_id), value=zigpy_list(zigpy_type, values)
    ).serialize()


def zigpy_field(type_id: int, zigpy_type: type, value: JsonValue) -> f.TypeValue:
    return f.TypeValue(type=t.uint8_t(type_id), value=zigpy_type(value))


def encode_coll(elem_type_id: int, payload: bytes, count: int) -> bytes:
    return bytes([elem_type_id]) + struct.pack("<H", count) + payload


def encode_coll_null(elem_type_id: int) -> bytes:
    return bytes([elem_type_id, 0xFF, 0xFF])


def encode_elements(values: Iterable[JsonValue], serialize: Serializer) -> bytes:
    return b"".join(serialize(value) for value in values)


def collection_invalids(
    elem_type_id: int,
    elem_size: int,
    serialize: Serializer,
    first_values: Sequence[JsonValue],
    wrong_elem_type_id: int,
) -> list[JsonObject]:
    first_payload = encode_elements(first_values, serialize)
    trailing_payload = first_payload + bytes(elem_size)
    return [
        invalid_case(
            "declared_count_too_large",
            encode_coll(elem_type_id, first_payload, len(first_values) + 1),
            "InsufficientBytes",
        ),
        invalid_case(
            "trailing_payload",
            encode_coll(elem_type_id, trailing_payload, 1),
            "InvalidLength",
        ),
        invalid_case(
            "wrong_element_type_null_count",
            encode_coll(wrong_elem_type_id, b"", 0xFFFF),
            "NullSentinel",
        ),
    ]


def collection_error_sections(
    elem_type_id: int,
    elem_size: int,
    serialize: Serializer,
    invalid_values: Sequence[JsonValue],
    wrong_elem_type_id: int,
) -> dict[str, list[JsonObject]]:
    return {
        "null_wire": [named_wire("null", encode_coll_null(elem_type_id))],
        "type_mismatch": [
            type_mismatch_case(
                "wrong_element_type",
                encode_coll(wrong_elem_type_id, bytes(elem_size), 1),
                expected=elem_type_id,
                found=wrong_elem_type_id,
            )
        ],
        "invalid": collection_invalids(
            elem_type_id, elem_size, serialize, invalid_values, wrong_elem_type_id
        ),
    }


def gen_collection(
    collection_type: type[f.TypedCollection],
    elem_type_id: int,
    elem_size: int,
    zigpy_type: type,
    sample_sets: Sequence[tuple[str, Sequence[JsonValue]]],
    wrong_elem_type_id: int,
) -> JsonObject:
    serialize = zigpy_serializer(zigpy_type)
    return fixture(
        roundtrip=[
            values_case(name, zigpy_collection(collection_type, elem_type_id, zigpy_type, values), values)
            for name, values in sample_sets
        ],
        **collection_error_sections(
            elem_type_id, elem_size, serialize, sample_sets[1][1], wrong_elem_type_id
        ),
    )


def gen_collection_set(
    elem_type_id: int,
    elem_size: int,
    zigpy_type: type,
    unique_sample_sets: Sequence[tuple[str, Sequence[JsonValue]]],
    duplicate_sample: tuple[str, Sequence[JsonValue]],
    wrong_elem_type_id: int,
) -> JsonObject:
    dup_name, dup_values = duplicate_sample
    serialize = zigpy_serializer(zigpy_type)
    return fixture(
        valid=[
            values_case(name, zigpy_collection(f.Set, elem_type_id, zigpy_type, values), values)
            for name, values in unique_sample_sets
        ],
        duplicate=[
            named_wire(
                dup_name,
                encode_coll(
                    elem_type_id,
                    encode_elements(dup_values, serialize),
                    len(dup_values),
                ),
            )
        ],
        **collection_error_sections(
            elem_type_id,
            elem_size,
            serialize,
            unique_sample_sets[0][1],
            wrong_elem_type_id,
        ),
    )


def pair_struct(u8_val: int, u16_val: int) -> f.ZCLStructure:
    return f.ZCLStructure(
        [
            zigpy_field(TYPE_U8, t.uint8_t, u8_val),
            zigpy_field(TYPE_U16, t.uint16_t, u16_val),
        ]
    )


def struct_header(field_count: int) -> bytes:
    return struct.pack("<H", field_count)


def gen_collection_struct_pair() -> JsonObject:
    samples = [
        ("nominal", 0x42, 0x1234),
        ("zeroes", 0x00, 0x0000),
        ("max_non_null", 0xFE, 0xFFFE),
    ]
    return fixture(
        roundtrip=[
            value_case(name, pair_struct(u8_val, u16_val).serialize(), [u8_val, u16_val])
            for name, u8_val, u16_val in samples
        ],
        null_wire=[named_wire("null", struct_header(0xFFFF))],
        invalid=[
            invalid_case(
                "declared_field_count_too_high",
                struct_header(3) + bytes([TYPE_U8, 0x42, TYPE_U16, 0x34, 0x12]),
                "UnconsumedData",
            ),
            invalid_case(
                "second_field_wrong_type",
                struct_header(2) + bytes([TYPE_U8, 0x42, TYPE_U8, 0x34]),
                "TypeIdMismatch",
                expected=TYPE_U16,
                found=TYPE_U8,
            ),
            invalid_case(
                "truncated_second_field",
                struct_header(2) + bytes([TYPE_U8, 0x42, TYPE_U16, 0x34]),
                "InsufficientBytes",
            ),
        ],
    )


def gen_nested_array_u8() -> JsonObject:
    array_type_id = TYPE_ARRAY
    u8_type_id = TYPE_U8

    def inner_array(values: list[int]) -> f.Array:
        return f.Array(type=u8_type_id, value=zigpy_list(t.uint8_t, values))

    def encode_inner(values: list[int]) -> bytes:
        return inner_array(values).serialize()

    samples = [
        ("empty", []),
        ("single_empty_inner", [[]]),
        ("two_inner_arrays", [[1, 2], [3]]),
    ]
    roundtrip = []
    for name, inner_lists in samples:
        inner_values = [inner_array(values) for values in inner_lists]
        inner_wires = [inner.serialize() for inner in inner_values]
        roundtrip.append(
            named_wire(
                name,
                f.Array(
                    type=array_type_id,
                    value=t.LVList[f.Array, t.uint16_t](inner_values),
                ).serialize(),
                inner_wires=[hex_wire(inner_wire) for inner_wire in inner_wires],
            )
        )

    return fixture(
        roundtrip=roundtrip,
        null_wire=[named_wire("null", encode_coll_null(array_type_id))],
        type_mismatch=[
            type_mismatch_case(
                "wrong_element_type",
                encode_coll(u8_type_id, bytes(1), 1),
                expected=array_type_id,
                found=u8_type_id,
            )
        ],
        invalid=[
            invalid_case(
                "truncated_inner_array",
                encode_coll(array_type_id, bytes([u8_type_id, 0x02, 0x00, 0x01]), 1),
                "InsufficientBytes",
            ),
            invalid_case(
                "trailing_payload",
                encode_coll(array_type_id, encode_inner([1]) + bytes([0x00]), 1),
                "InvalidLength",
            ),
        ],
    )


def gen_array_of_struct_pair() -> JsonObject:
    struct_type_id = TYPE_STRUCT
    samples = [
        ("empty", []),
        ("single", [(0x42, 0x1234)]),
        ("multiple", [(0, 0), (0x42, 0x1234)]),
    ]
    roundtrip = []
    for name, pairs in samples:
        structs = [pair_struct(u8_val, u16_val) for u8_val, u16_val in pairs]
        wire = f.Array(
            type=struct_type_id, value=t.LVList[f.ZCLStructure, t.uint16_t](structs)
        ).serialize()
        roundtrip.append(
            values_case(name, wire, [[u8_val, u16_val] for u8_val, u16_val in pairs])
        )

    return fixture(
        roundtrip=roundtrip,
        null_wire=[named_wire("null", encode_coll_null(struct_type_id))],
        type_mismatch=[
            type_mismatch_case(
                "wrong_element_type",
                encode_coll(TYPE_U8, bytes(1), 1),
                expected=struct_type_id,
                found=TYPE_U8,
            )
        ],
        invalid=[
            invalid_case(
                "truncated_struct_element",
                encode_coll(
                    struct_type_id,
                    bytes([0x02, 0x00, TYPE_U8, 0x42, TYPE_U16, 0x34]),
                    1,
                ),
                "InsufficientBytes",
            ),
            invalid_case(
                "trailing_payload",
                encode_coll(
                    struct_type_id, pair_struct(0x42, 0x1234).serialize() + bytes([0x00]), 1
                ),
                "InvalidLength",
            ),
        ],
    )


def gen_struct_with_array() -> JsonObject:
    u8_type_id = TYPE_U8
    array_type_id = TYPE_ARRAY

    def array_value(arr_elems: list[int]) -> f.Array:
        return f.Array(type=u8_type_id, value=zigpy_list(t.uint8_t, arr_elems))

    def encode_struct(u8_val: int, arr_elems: list[int]) -> bytes:
        return f.ZCLStructure(
            [
                zigpy_field(u8_type_id, t.uint8_t, u8_val),
                f.TypeValue(
                    type=t.uint8_t(array_type_id), value=array_value(arr_elems)
                ),
            ]
        ).serialize()

    def struct_prefix(field_count: int = 2) -> bytes:
        return struct_header(field_count) + bytes([u8_type_id, 0x2A, array_type_id])

    samples = [
        ("empty_array", 0, []),
        ("nominal", 42, [1, 2, 3]),
        ("max_u8", 254, [10, 20]),
    ]
    return fixture(
        roundtrip=[
            value_case(
                name,
                encode_struct(u8_val, arr_elems),
                {"u8": u8_val, "array": arr_elems},
            )
            for name, u8_val, arr_elems in samples
        ],
        null_wire=[named_wire("null", struct_header(0xFFFF))],
        invalid=[
            invalid_case(
                "array_field_truncated",
                struct_prefix() + encode_coll(u8_type_id, bytes([0x01, 0x02]), 3),
                "InsufficientBytes",
            ),
            invalid_case(
                "array_field_wrong_element_type",
                struct_prefix() + encode_coll(TYPE_U16, b"", 0),
                "TypeIdMismatch",
                expected=u8_type_id,
                found=TYPE_U16,
            ),
            invalid_case(
                "declared_field_count_too_high",
                struct_prefix(3) + encode_coll(u8_type_id, b"", 0),
                "UnconsumedData",
            ),
        ],
    )


def generator(data_type: f.DataTypeId, gen_fn: Callable[[], JsonObject]) -> Generator:
    return int(zcl_type_id(data_type)), gen_fn


def int_generator(
    data_type: f.DataTypeId, zigpy_type: type, bits: int, *, signed: bool
) -> Generator:
    return generator(data_type, lambda: gen_int(zigpy_type, bits, signed=signed))


def float_generator(data_type: f.DataTypeId, zigpy_type: type, width: int) -> Generator:
    return generator(data_type, lambda: gen_float(zigpy_type, width))


def bitmap_generator(data_type: f.DataTypeId, zigpy_type: type, bits: int) -> Generator:
    return generator(data_type, lambda: gen_bitmap(zigpy_type, bits))


def collection_generator(
    collection_type_id: int,
    collection_type: type[f.TypedCollection],
    elem_type_id: int,
    elem_size: int,
    zigpy_type: type,
    sample_sets: Sequence[tuple[str, Sequence[JsonValue]]],
    wrong_elem_type_id: int,
) -> Generator:
    return int(collection_type_id), lambda: gen_collection(
        collection_type,
        elem_type_id,
        elem_size,
        zigpy_type,
        sample_sets,
        wrong_elem_type_id,
    )


def set_generator(
    elem_type_id: int,
    elem_size: int,
    zigpy_type: type,
    unique_sample_sets: Sequence[tuple[str, Sequence[JsonValue]]],
    duplicate_sample: tuple[str, Sequence[JsonValue]],
    wrong_elem_type_id: int,
) -> Generator:
    return int(TYPE_SET), lambda: gen_collection_set(
        elem_type_id,
        elem_size,
        zigpy_type,
        unique_sample_sets,
        duplicate_sample,
        wrong_elem_type_id,
    )


GENERATORS: dict[str, Generator] = {
    "u8": int_generator(f.DataTypeId.uint8, t.uint8_t, 8, signed=False),
    "u16": int_generator(f.DataTypeId.uint16, t.uint16_t, 16, signed=False),
    "u32": int_generator(f.DataTypeId.uint32, t.uint32_t, 32, signed=False),
    "u64": int_generator(f.DataTypeId.uint64, t.uint64_t, 64, signed=False),
    "i8": int_generator(f.DataTypeId.int8, t.int8s, 8, signed=True),
    "i16": int_generator(f.DataTypeId.int16, t.int16s, 16, signed=True),
    "i32": int_generator(f.DataTypeId.int32, t.int32s, 32, signed=True),
    "i64": int_generator(f.DataTypeId.int64, t.int64s, 64, signed=True),
    "bool": (int(TYPE_BOOL), gen_bool),
    "f32": float_generator(f.DataTypeId.single, t.Single, 32),
    "f64": float_generator(f.DataTypeId.double, t.Double, 64),
    "bitmap8": bitmap_generator(f.DataTypeId.map8, t.bitmap8, 8),
    "bitmap16": bitmap_generator(f.DataTypeId.map16, t.bitmap16, 16),
    "bitmap32": bitmap_generator(f.DataTypeId.map32, t.bitmap32, 32),
    "bitmap64": bitmap_generator(f.DataTypeId.map64, t.bitmap64, 64),
    "enum8": generator(
        f.DataTypeId.enum8,
        lambda: gen_enum(
            t.enum8,
            [
                ("zero", 0x00),
                ("one", 0x01),
                ("arbitrary", 0x42),
                ("max_non_null", 0xFE),
            ],
            0xFF,
        ),
    ),
    "enum16": generator(
        f.DataTypeId.enum16,
        lambda: gen_enum(
            t.enum16,
            [
                ("zero", 0x0000),
                ("one", 0x0001),
                ("byte_boundary", 0x0100),
                ("max_non_null", 0xFFFE),
            ],
            0xFFFF,
        ),
    ),
    "short_text": generator(f.DataTypeId.string, gen_short_text),
    "short_octet_string": generator(f.DataTypeId.octstr, gen_short_octet_string),
    "coll_array_u8": collection_generator(
        TYPE_ARRAY,
        f.Array,
        TYPE_U8,
        1,
        t.uint8_t,
        [("empty", []), ("single", [42]), ("multiple", [1, 100, 0xFE])],
        TYPE_U16,
    ),
    "coll_array_u16": collection_generator(
        TYPE_ARRAY,
        f.Array,
        TYPE_U16,
        2,
        t.uint16_t,
        [("empty", []), ("single", [1]), ("multiple", [1, 2, 3])],
        TYPE_U8,
    ),
    "coll_array_i16": collection_generator(
        TYPE_ARRAY,
        f.Array,
        TYPE_I16,
        2,
        t.int16s,
        [("empty", []), ("single_negative", [-1]), ("mixed", [0, 100, -100])],
        TYPE_U8,
    ),
    "coll_array_bool": collection_generator(
        TYPE_ARRAY,
        f.Array,
        TYPE_BOOL,
        1,
        t.Bool,
        [("empty", []), ("single_true", [True]), ("mixed", [False, True, False])],
        TYPE_U8,
    ),
    "coll_bag_u8": collection_generator(
        TYPE_BAG,
        f.Bag,
        TYPE_U8,
        1,
        t.uint8_t,
        [
            ("empty", []),
            ("single", [42]),
            ("multiple", [1, 100, 0xFE]),
            ("duplicates_allowed", [1, 1, 2]),
        ],
        TYPE_U16,
    ),
    "coll_bag_u16": collection_generator(
        TYPE_BAG,
        f.Bag,
        TYPE_U16,
        2,
        t.uint16_t,
        [
            ("empty", []),
            ("single", [1]),
            ("multiple", [1, 2, 3]),
            ("duplicates_allowed", [1, 1, 2]),
        ],
        TYPE_U8,
    ),
    "coll_set_u8": set_generator(
        TYPE_U8,
        1,
        t.uint8_t,
        [("two_unique", [1, 2]), ("three_unique", [10, 11, 12])],
        ("duplicate", [1, 1]),
        TYPE_U16,
    ),
    "coll_set_u16": set_generator(
        TYPE_U16,
        2,
        t.uint16_t,
        [("two_unique", [1, 2]), ("three_unique", [10, 20, 30])],
        ("duplicate", [1, 1]),
        TYPE_U8,
    ),
    "coll_struct_pair": (int(TYPE_STRUCT), gen_collection_struct_pair),
    "coll_nested_array_u8": (int(TYPE_ARRAY), gen_nested_array_u8),
    "coll_array_of_struct_pair": (int(TYPE_ARRAY), gen_array_of_struct_pair),
    "coll_struct_with_array": (int(TYPE_STRUCT), gen_struct_with_array),
}


def build_fixture() -> str:
    data = {
        "schema_version": 1,
        "source": {
            "generator": SOURCE,
            "zigpy": ZIGPY_VERSION,
        },
        "types": {
            name: {"type_id": int(type_id), **gen_fn()}
            for name, (type_id, gen_fn) in GENERATORS.items()
        },
    }
    return json.dumps(data, indent=2, ensure_ascii=False) + "\n"


def write_or_check(path: Path, content: str, check: bool) -> bool:
    if check:
        existing = path.read_text(encoding="utf-8") if path.exists() else ""
        if existing == content:
            return True
        diff = difflib.unified_diff(
            existing.splitlines(keepends=True),
            content.splitlines(keepends=True),
            fromfile=str(path),
            tofile="generated",
        )
        sys.stderr.write("".join(diff))
        return False

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path, help="Output directory")
    parser.add_argument(
        "--check",
        action="store_true",
        help="Diff against existing fixtures; exit 1 if any differ",
    )
    args = parser.parse_args()

    ok = True
    content = build_fixture()
    dest = args.out / OUTPUT_FILE

    if write_or_check(dest, content, args.check):
        if not args.check:
            print(f"  wrote {dest}")
    else:
        print(f"  DRIFT {dest}", file=sys.stderr)
        ok = False

    if not ok and args.check:
        print(
            "\nVectors out of date — run `uv run scripts/gen_vectors.py --out zigbee-cluster-library/tests/vectors/` to refresh.",
            file=sys.stderr,
        )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
