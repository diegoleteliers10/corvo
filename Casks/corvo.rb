cask "corvo" do
  arch arm: "aarch64", intel: "x86_64"

  version "0.1.1"
  sha256 arm:   "a2578efab267ee81c0bff7ec588b51959aceab8249960dc3d12304cbc8febe11",
         intel: "8707d0cc5f9f6b5524f35d924e148f76beb51aaf0bac63a5c58371296698ae46"

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
