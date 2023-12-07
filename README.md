# It must be invoked as a cargo subcommand
# All options after `--` are passed to cargo itself
# We need to point it at our library since it contains
# all our SQL queries.
cargo sqlx prepare -- --lib