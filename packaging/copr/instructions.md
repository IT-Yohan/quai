**Fedora Workstation, KDE Plasma and other spins**

```
sudo dnf copr enable @OWNER@/@PROJECT@
sudo dnf install quai
```

On a desktop the window comes too (`quai-gui`, with the GTK or Kirigami frontend that fits your desktop). Open **Quai** from the menu, or use the command: `quai add owner/repo`.

**Fedora Silverblue, Kinoite and other Atomic desktops**

```
sudo curl -o /etc/yum.repos.d/quai.repo https://copr.fedorainfracloud.org/coprs/@OWNER@/@PROJECT@/repo/fedora-$(rpm -E %fedora)/@OWNER@-@PROJECT@-fedora-$(rpm -E %fedora).repo
rpm-ostree install quai quai-gui
systemctl reboot
```

Updates of Quai itself then arrive with the rest of the system. Packages are also attached to each [GitHub release](https://github.com/IT-Yohan/quai/releases).
