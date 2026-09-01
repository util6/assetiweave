#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if assetiweave_lib::has_memory_recall_mcp_stdio_arg() {
        assetiweave_lib::run_memory_recall_mcp_stdio();
    } else if assetiweave_lib::has_team_mcp_stdio_arg() {
        assetiweave_lib::run_team_mcp_stdio();
    } else {
        assetiweave_lib::run();
    }
}
