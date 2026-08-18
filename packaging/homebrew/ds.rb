class Ds < Formula
  desc "Check domain availability over RDAP with a WHOIS fallback"
  homepage "https://github.com/aminulbd/ds"
  url "https://github.com/aminulbd/ds/archive/refs/tags/v0.1.5.tar.gz"
  sha256 "458549fe0897c6cafe3b96abb923057fdaf65f57cfa008aab6bfe82ce655cf0d"
  license "MIT"
  head "https://github.com/aminulbd/ds.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
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
