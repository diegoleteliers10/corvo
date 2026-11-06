cask "corvo" do
  arch arm: "aarch64", intel: "x86_64"

  version "0.6.2"
  sha256 arm:   "690ecc655ad2d886bbfe2b33b3fb787c02198449aa03192827cb4cef316c27c8",
         intel: "b474c7dd2d7500ba18579267a5920a45b2b0ca4424c30f1bad0414c57da1932e"

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
