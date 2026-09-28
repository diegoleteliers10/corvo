cask "corvo" do
  arch arm: "aarch64", intel: "x86_64"

  version "0.2.0"
  sha256 arm:   "943fec12d7956290ae4a6731c204ddf183a2a1c210341cd573463326ba2b8238",
         intel: "358beaf1fedb3569c988cc6e48f24e6508199eeb327dc3a4165682eb82b735f8"

  url "https://github.com/diegoleteliers10/corvo/releases/download/v#{version}/corvo-#{arch}-apple-darwin.dmg"
  name "Corvo"
  desc "Lightweight native application launcher built with Rust & GPUI"
  homepage "https://github.com/diegoleteliers10/corvo"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true

  app "Corvo.app"
  binary "#{appdir}/Corvo.app/Contents/MacOS/corvo"

  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-cr", "#{appdir}/Corvo.app"],
                   sudo: false
    system_command "/usr/bin/codesign",
                   args: ["--force", "--deep", "-s", "-", "#{appdir}/Corvo.app"],
                   sudo: false
  end

  zap trash: [
    "~/Library/Application Support/corvo",
    "~/Library/Caches/corvo",
    "~/Library/Preferences/sh.corvo.corvo.plist",
    "~/Library/Saved Application State/sh.corvo.corvo.savedState",
  ]
end
