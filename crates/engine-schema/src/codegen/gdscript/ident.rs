pub(super) fn gd_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "elif"
            | "else"
            | "for"
            | "while"
            | "match"
            | "when"
            | "break"
            | "continue"
            | "pass"
            | "return"
            | "class"
            | "class_name"
            | "extends"
            | "is"
            | "in"
            | "as"
            | "self"
            | "super"
            | "signal"
            | "func"
            | "static"
            | "const"
            | "enum"
            | "var"
            | "breakpoint"
            | "preload"
            | "await"
            | "yield"
            | "assert"
            | "void"
            | "true"
            | "false"
            | "null"
            | "and"
            | "or"
            | "not"
            | "export"
            | "onready"
            | "tool"
            | "remote"
            | "master"
            | "puppet"
            | "remotesync"
            | "mastersync"
            | "puppetsync"
            | "sync"
    )
}

pub(super) fn gd_ident(name: &str) -> String {
    if gd_keyword(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}
