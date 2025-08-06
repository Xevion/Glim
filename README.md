<div align="center">
  <img src="https://gh.xevion.dev/Xevion/Glim.png?s=1.2" alt="Glim repository card">
</div>

# Glim

[![Tests Status][badge-test]][test] [![Online Demo][badge-online-demo]][demo] [![Last Commit][badge-last-commit]][commits]

[badge-test]: https://github.com/Xevion/Glim/actions/workflows/test.yaml/badge.svg
[badge-online-demo]: https://img.shields.io/github/deployments/Xevion/Glim/production?logo=railway&label=deploy&labelColor=13111c
[badge-last-commit]: https://img.shields.io/github/last-commit/Xevion/Glim
[test]: https://github.com/Xevion/Glim/actions/workflows/test.yaml
[demo]: https://gh.xevion.dev/Xevion/Glim.png
[commits]: https://github.com/Xevion/Glim/commits/master

<!-- [badge-coverage]: https://coveralls.io/repos/github/Xevion/Glim/badge.svg?branch=master -->
<!-- [coverage]: https://coveralls.io/github/Xevion/Glim?branch=master -->

Glīm (pronounced 'gleem') is a command-line tool and web-server for generating beautiful, dynamic cards for your GitHub repositories.

The name comes from an Old English word meaning a small gleam of light, representing how each card captures the brilliance of your project.

- Built entirely with Rust, generating in <50ms, idling at ~5 MB of memory
- Uses the [GitHub API](https://docs.github.com/en/rest) to fetch repository data
- Provides image rasterization and encoding to PNG, JPEG, AVIF, WebP, GIF
- Fully tested, built for every major OS and architecture

## Usage

```bash
Usage: glim [OPTIONS] [REPOSITORY]

Arguments:
  [REPOSITORY]  The repository to generate a card for, in the format `owner/repo`

Options:
  -o, --output <OUTPUT>                         The output path for the generated card
  -t, --token <TOKEN>                           GitHub token to use for API requests
  -s, --server [<HOST:PORT[,HOST:PORT[,...]]>]  Start the HTTP server
  -L, --log-level <LEVEL>                       Set the logging level [default: DEBUG]
  -h, --help                                    Print help
  -V, --version                                 Print version
```

### Environment Variables

- `GITHUB_TOKEN`: To avoid rate-limiting, you can provide a GitHub personal access token through this environment variable.

When creating a GitHub personal access token for Glim, **do not add any scopes**.

- A fine-grained personal access token with **no scopes** against **public repositories** is ideal.
- Given that private repositories generally are not starred or forked often, there is no reason to use a token with `repo` scope, and no reason to provide access to private repositories.

For most users, **no token is required** as Glim works perfectly with public repositories using anonymous API access.

If you'd like to use a token anyways, you can create one in the **Settings** > **Developer settings** > **Personal access tokens** > [Fine-grained tokens](https://github.com/settings/personal-access-tokens) page. I strongly recommend that you do not click on any scopes, and do not change the default Repository access from 'Public repositories'.

## License

This project is licensed under the MIT License.
