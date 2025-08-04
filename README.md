# Livecards

A command-line tool to generate beautiful, dynamic cards for your GitHub repositories.

- Built completely with Rust
- Uses the [GitHub API](https://docs.github.com/en/rest) to fetch repository data
- Generates cards from SVGs, rasterized to PNGs

## Usage

```bash
Usage: livecards [OPTIONS] <REPOSITORY>

Arguments:
  <REPOSITORY>  The repository to generate a card for, in the format `owner/repo`

Options:
  -o, --output <OUTPUT>  The output path for the generated card
  -h, --help             Print help
  -V, --version          Print version
```

### Environment Variables

- `GITHUB_TOKEN`: To avoid rate-limiting, you can provide a GitHub personal access token through this environment variable.

## Building

To build the project from source, you'll need to have Rust and Cargo installed.

1.  Clone the repository:
    ```bash
    git clone https://github.com/Xevion/livecards.git
    ```
2.  Build the project:
    ```bash
    cargo build --release
    ```
3.  The executable will be located in `target/release/livecards`.

## Contributing

Contributions are welcome! Please feel free to open an issue or submit a pull request.

## License

This project is licensed under the MIT License.
