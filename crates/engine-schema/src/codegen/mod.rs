mod assembly;
mod common;
mod cpp;
mod csharp;
mod gdscript;
mod lua;

#[cfg(test)]
mod tests;

pub use assembly::emit_as;

use crate::{RootState, SchemaDef};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    CSharp,
    Gdscript,
    Cpp,
    Lua,

    As,
}

impl Language {
    pub const ALL: [Language; 5] = [
        Language::CSharp,
        Language::Gdscript,
        Language::Cpp,
        Language::Lua,
        Language::As,
    ];

    pub fn parse(text: &str) -> Option<Language> {
        match text {
            "cs" | "csharp" | "c#" => Some(Language::CSharp),
            "gd" | "gdscript" | "godot" => Some(Language::Gdscript),
            "cpp" | "c++" | "cxx" => Some(Language::Cpp),
            "lua" | "luajit" => Some(Language::Lua),
            "as" | "assemblyscript" => Some(Language::As),
            _ => None,
        }
    }

    pub fn file_name(self) -> &'static str {
        match self {
            Language::CSharp => "CYGSchemaGenerated.cs",
            Language::Gdscript => "cyg_schema_generated.gd",
            Language::Cpp => "cyg_schema_generated.hpp",
            Language::Lua => "cyg_schema_generated.lua",
            Language::As => "cyg.schema.ts",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Language::CSharp => "C#",
            Language::Gdscript => "GDScript",
            Language::Cpp => "C++",
            Language::Lua => "Lua",
            Language::As => "AssemblyScript",
        }
    }

    pub fn flag(self) -> &'static str {
        match self {
            Language::CSharp => "cs",
            Language::Gdscript => "gdscript",
            Language::Cpp => "cpp",
            Language::Lua => "lua",
            Language::As => "as",
        }
    }
}

pub fn emit(
    language: Language,
    schemas: &[SchemaDef],
    root: Option<&RootState>,
    source_name: &str,
) -> String {
    let source_name = common::sanitize_source(source_name);
    let root = root.cloned();
    match language {
        Language::CSharp => csharp::emit_csharp(schemas, root.as_ref(), &source_name),
        Language::As => assembly::emit_as(schemas, root.as_ref(), &source_name),
        Language::Gdscript => gdscript::emit_gdscript(schemas, root.as_ref(), &source_name),
        Language::Cpp => cpp::emit_cpp(schemas, root.as_ref(), &source_name),
        Language::Lua => lua::emit_lua(schemas, root.as_ref(), &source_name),
    }
}
