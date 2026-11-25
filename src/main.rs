fn main() {
    std::process::exit(godot_llm_lint::app::handlers::handle_cli(
        std::env::args().skip(1).collect(),
    ));
}
