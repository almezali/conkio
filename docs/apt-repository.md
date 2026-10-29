# Conkio APT repository

The GitHub Actions workflow publishes a signed APT repository to `https://almezali.github.io/conkio/`, using the `.deb` assets attached to GitHub Releases. Supported package architectures are `amd64` and `arm64`.


## Install on Debian, Ubuntu, Linux Mint, or Pop!_OS


```bash
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://almezali.github.io/conkio/public.key | sudo gpg --dearmor --yes -o /etc/apt/keyrings/conkio.gpg
sudo chmod 0644 /etc/apt/keyrings/conkio.gpg
echo "deb [arch=amd64,arm64 signed-by=/etc/apt/keyrings/conkio.gpg] https://almezali.github.io/conkio stable main" | sudo tee /etc/apt/sources.list.d/conkio.list >/dev/null
sudo apt update
sudo apt install conkio
```

Conky itself is required too: `sudo apt install conky-all`.
