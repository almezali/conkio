# Conkio APT repository

The GitHub Actions workflow publishes a signed APT repository to `https://almezali.github.io/conkio/`, using the `.deb` assets attached to GitHub Releases. Supported package architectures are `amd64` and `arm64`.

## One-time maintainer setup

1. Open **Settings → Pages** and select **GitHub Actions** as the deployment source.
2. Open **Settings → Secrets and variables → Actions** and add `APT_GPG_PRIVATE_KEY` with an ASCII-armored private GPG signing key. If it is protected by a passphrase, add `APT_GPG_PASSPHRASE` too.
3. Never commit the private key or passphrase to the repository.
4. Publish a GitHub Release with `.deb` assets, or start **Actions → Publish APT repository → Run workflow**.

The workflow publishes the public key at `public.key`. Verify the key fingerprint through a trusted channel before trusting it.

## Install on Debian, Ubuntu, Linux Mint, or Pop!_OS

Run these commands only after the first deployment has completed successfully:

```bash
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://almezali.github.io/conkio/public.key | sudo gpg --dearmor --yes -o /etc/apt/keyrings/conkio.gpg
sudo chmod 0644 /etc/apt/keyrings/conkio.gpg
echo "deb [arch=amd64,arm64 signed-by=/etc/apt/keyrings/conkio.gpg] https://almezali.github.io/conkio stable main" | sudo tee /etc/apt/sources.list.d/conkio.list >/dev/null
sudo apt update
sudo apt install conkio
```

Conky itself is required too: `sudo apt install conky-all`.
