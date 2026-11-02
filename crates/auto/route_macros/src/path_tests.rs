use crate::docs::join_paths;

#[test]
fn joins_and_converts_spring_style_parameters() {
    assert_eq!(join_paths("/users/", "/:id"), "/users/{id}");
    assert_eq!(join_paths("", ""), "/");
    assert_eq!(join_paths("/files", "/*path"), "/files/{*path}");
}
