# Conkio APT repository setup

The publishing workflow will host a signed APT repository at `https://almezali.github.io/conkio/` and use Debian packages attached to GitHub Releases for `amd64` and `arm64`.

## Maintainer setup

1. In Settings → Pages, choose GitHub Actions as the deployment source.
2. In Settings → Secrets and variables → Actions, add `APT_GPG_PRIVATE_KEY` containing the ASCII-armored private signing key. Add `APT_GPG_PASSPHRASE` if the key has a passphrase.
3. Never commit the private key or passphrase.
4. Run the publishing workflow or publish a GitHub Release containing `.deb` assets.

The public signing key is published as `public.key`. Users should verify its fingerprint through a trusted channel before trusting it.
