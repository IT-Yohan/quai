**Quai** keeps the RPM packages that projects publish on **GitHub, GitLab or Codeberg releases** up to date with dnf, GNOME Software and Discover.

Many applications ship their Fedora packages only as `.rpm` files attached to their releases. Installed by hand, they never receive updates. Quai turns each project you follow into a small signed dnf repository served on your own machine (`127.0.0.1`), so these packages install, update and roll back like any other Fedora package, on Fedora Workstation, KDE Plasma and the Atomic desktops.

- Add a project by typing `owner/repo` or pasting its URL, and see what it ships before adding it
- Metadata signed with a key local to your machine; packages checked against the forge's SHA-256
- Guards against packages that would replace system packages (Obsoletes, other repositories)
- A native window on GNOME (GTK 4 / libadwaita) and on KDE Plasma (Kirigami), and the `quai` command

Source code, issues and documentation: [github.com/IT-Yohan/quai](https://github.com/IT-Yohan/quai)
