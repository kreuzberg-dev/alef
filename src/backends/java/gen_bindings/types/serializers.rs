use crate::core::hash::{self, CommentStyle};

pub(crate) fn gen_byte_array_serializer(package: &str) -> String {
    let header = hash::header(CommentStyle::DoubleSlash);
    let imports = [
        "com.fasterxml.jackson.core.JsonGenerator",
        "com.fasterxml.jackson.databind.SerializerProvider",
        "com.fasterxml.jackson.databind.ser.std.StdSerializer",
    ];
    let mut out = crate::backends::java::template_env::render(
        "java_file_header.jinja",
        minijinja::context! { header => header, package => package, imports => &imports },
    );
    out.push('\n');
    out.push_str(&crate::backends::java::template_env::render(
        "byte_array_serializer.jinja",
        minijinja::context! {},
    ));
    out
}
