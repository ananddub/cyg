pub fn live_statement(table: &str) -> String {
    format!("LIVE SELECT * FROM {table} WHERE game_id = $game")
}
