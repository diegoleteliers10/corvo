cask "corvo" do
  arch arm: "aarch64", intel: "x86_64"

  version "0.7.2"
  sha256 arm:   "0fb805bf7a37115fe319ef1a48321a290ca5ffc67e891d1a5b99d59781869d7a",
         intel: "0f86690cbc2a2c07523056f59929f677f9350b122701c2f535b333ff968ee3ee"

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
