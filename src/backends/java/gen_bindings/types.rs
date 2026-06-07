mod builders;
mod enums;
mod opaque;
mod records;
mod serializers;
mod shared;

pub(crate) use enums::gen_enum_class;
pub(crate) use opaque::gen_opaque_handle_class;
pub(crate) use records::gen_record_type;
pub(crate) use serializers::gen_byte_array_serializer;

#[cfg(test)]
mod tests;
