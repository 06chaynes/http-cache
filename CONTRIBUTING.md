# Contribution guidelines

First off, thank you for considering contributing to http-cache.

If your contribution is not straightforward, please first discuss the change you
wish to make by creating a new [issue](https://github.com/06chaynes/http-cache/issues) 
or [discussion](https://github.com/06chaynes/http-cache/discussions) before making the change.

## Reporting issues

Before reporting an issue on the
[issue tracker](https://github.com/06chaynes/http-cache/issues),
please check that it has not already been reported by searching for some related
keywords.

## Pull requests

## Developing

### Set up

This is no different from other Rust projects.

```shell
git clone https://github.com/06chaynes/http-cache
cd http-cache
cargo test
```

### Useful Commands

- Run Clippy:

  ```shell
  cargo clippy --all-targets --all-features -- -D warnings
  ```

- Check to see if there are code formatting issues

  ```shell
  cargo fmt --all -- --check
  ```

- Format the code in the project

  ```shell
  cargo fmt --all
  ```
