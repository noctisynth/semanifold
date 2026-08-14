use napi_derive::napi;

#[napi]
pub fn run_cli(args: Vec<String>) -> u32 {
    node_exit_code(semifold::run_cli_with_args(cli_arguments(args)))
}

fn cli_arguments(args: Vec<String>) -> Vec<String> {
    std::iter::once("semifold".to_string())
        .chain(args)
        .collect()
}

fn node_exit_code(exit_code: u8) -> u32 {
    u32::from(exit_code)
}

#[cfg(test)]
mod tests {
    use super::{cli_arguments, node_exit_code};

    #[test]
    fn node_arguments_are_prefixed_with_the_rust_program_name() {
        assert_eq!(
            cli_arguments(vec!["status".to_string(), "--debug".to_string()]),
            ["semifold", "status", "--debug"]
        );
    }

    #[test]
    fn rust_exit_codes_are_preserved_for_node() {
        assert_eq!(node_exit_code(0), 0);
        assert_eq!(node_exit_code(1), 1);
        assert_eq!(node_exit_code(u8::MAX), u32::from(u8::MAX));
    }
}
