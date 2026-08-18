class Ds < Formula
  desc "Check domain availability over RDAP with a WHOIS fallback"
  homepage "https://github.com/aminulbd/ds"
  url "https://github.com/aminulbd/ds/archive/refs/tags/v0.1.5.tar.gz"
  sha256 "458549fe0897c6cafe3b96abb923057fdaf65f57cfa008aab6bfe82ce655cf0d"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  # Released versions install a prebuilt binary; `--HEAD` builds from source.
  head do
    url "https://github.com/aminulbd/ds.git", branch: "main"
    depends_on "rust" => :build
  end

  on_macos do
    on_arm do
      url "https://github.com/aminulbd/ds/releases/download/v0.1.5/ds-v0.1.5-aarch64-apple-darwin.tar.gz"
      sha256 "fb3eae63c218253d887ff6545fef594cddf16f1446ed02688ede049fd5d8c0e4"
    end
    on_intel do
      url "https://github.com/aminulbd/ds/releases/download/v0.1.5/ds-v0.1.5-x86_64-apple-darwin.tar.gz"
      sha256 "6cbc2a35162f7d1b7071a45bd9dec84ef2ac48ae8e307d0fd5d61abfdc3f71f5"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/aminulbd/ds/releases/download/v0.1.5/ds-v0.1.5-aarch64-unknown-linux-musl.tar.gz"
      sha256 "4f81b676d8a22631bc060abe4a5e8d5ef71ff17c5ccda4dbadcf96ea6ef28a5d"
    end
    on_intel do
      url "https://github.com/aminulbd/ds/releases/download/v0.1.5/ds-v0.1.5-x86_64-unknown-linux-musl.tar.gz"
      sha256 "239f7705af28f186211f1e04a5a87905e540cbf0ad69d01b660bedaba278807f"
    end
  end

  def install
    if build.head?
      system "cargo", "install", *std_cargo_args
    else
      bin.install "ds"
    end
    man1.install "ds.1"
  end

  test do
    assert_match "ds #{version}", shell_output("#{bin}/ds --version")

    # Argument handling, without touching the network.
    output = shell_output("#{bin}/ds apple --tld @#{testpath}/missing.txt 2>&1", 2)
    assert_match "reading TLD list", output

    output = shell_output("#{bin}/ds 2>&1", 2)
    assert_match "required arguments were not provided", output
  end
end
