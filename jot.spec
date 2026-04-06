Name:           jot
Version:        0.1.0
Release:        1%{?dist}
Summary:        A lightning-fast, terminal-native note-taking tool built in Rust.

License:        MIT
URL:            https://github.com/dev-Aatif/jot
Source0:        %{url}/archive/v%{version}.tar.gz

BuildRequires:  rust
BuildRequires:  cargo
BuildRequires:  sqlite-devel

%description
Jot is a fast, terminal-native note-taking tool that lets you capture,
retrieve, and search text snippets without leaving your shell.

%prep
%autosetup

%build
cargo build --release --locked --all-features

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_bindir}
install -p -m 755 target/release/jot %{buildroot}%{_bindir}/jot

%files
%license LICENSE
%doc README.md
%{_bindir}/jot

%changelog
* Mon Apr 06 2026 dev-Aatif <dev-Aatif@github.com> - 0.1.0-1
- Initial release of jot v0.1.0
