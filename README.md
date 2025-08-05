<div align="center">
  <img src="https://gh.xevion.dev/Xevion/Glim.png?s=1.2" alt="Glim">
</div>

# Glim

Glīm (pronounced 'gleem') is a command-line tool and web-server for generating beautiful, dynamic cards for your GitHub repositories.

The name comes from an Old English word meaning a small gleam of light, representing how each card captures the brilliance of your project.

- Built entirely with Rust, generating in <50ms, idling at ~5 MB of memory
- Uses the [GitHub API](https://docs.github.com/en/rest) to fetch repository data
- Provides image rasterization and encoding to PNG, JPEG, AVIF, WebP, GIF
- Fully tested, built for every major OS and architecture

## Usage

```bash
Usage: glim [OPTIONS] <REPOSITORY>

Arguments:
  <REPOSITORY>  The repository to generate a card for, in the format `owner/repo`

Options:
  -o, --output <OUTPUT>  The output path for the generated card
  -t, --token <TOKEN>    GitHub token to use for API requests
  -h, --help             Print help
  -V, --version          Print version
```

### Environment Variables

- `GITHUB_TOKEN`: To avoid rate-limiting, you can provide a GitHub personal access token through this environment variable.

## Building

To build the project from source, you'll need to have Rust and Cargo installed.

1.  Clone the repository:
    ```bash
    git clone https://github.com/Xevion/glim.git
    cd glim
    cargo build --release
    ```
2.  The executable will be located in `target/release/glim`.

## Contributing

Contributions are welcome! Please feel free to open an issue or submit a pull request.

## License

This project is licensed under the MIT License.
