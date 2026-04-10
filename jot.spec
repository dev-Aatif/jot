Name:           jotun
Version:        0.2.0
Release:        1%{?dist}
Summary:        A lightning-fast, terminal-native note-taking tool built in Rust.

License:        MIT
URL:            https://github.com/dev-Aatif/jot
Source0:        %{url}/archive/v%{version}.tar.gz

BuildRequires:  rust
BuildRequires:  cargo
BuildRequires:  sqlite-devel

%description
Jotun is a fast, terminal-native note-taking tool that lets you capture,
retrieve, and search text snippets without leaving your shell.

%prep
%autosetup

%build
cargo build --release --locked --all-features

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_bindir}
install -p -m 755 target/release/jotun %{buildroot}%{_bindir}/jotun

%files
%license LICENSE
%doc README.md
%{_bindir}/jotun

%changelog
* Sat Apr 11 2026 dev-Aatif <dev-Aatif@github.com> - 0.2.0-1
- Release of jotun v0.2.0 with Interactive TUI Dashboard
* Mon Apr 06 2026 dev-Aatif <dev-Aatif@github.com> - 0.1.0-1
- Initial release of jotun v0.1.0
