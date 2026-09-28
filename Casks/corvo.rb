cask "corvo" do
  arch arm: "aarch64", intel: "x86_64"

  version "0.1.7"
  sha256 arm:   "3593f1ab69e3c52f1bee5ae44a9b0f5f60950fb8436762f9ee9f13c875c9852c",
         intel: "433c1ea3cf865ba07a26f1aa862e280461fa8acf70ed2cdf4d2e0587305406d5"

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
