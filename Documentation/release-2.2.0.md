# OpenNaga 2.2.0

NagaController is now OpenNaga, an independent project, and the interface is in English.

- New name: the app, window, menu bar item and DMG are called OpenNaga. The bundle identifier and the profiles folder are unchanged, so existing profiles load as before.
- English interface: every section, action, system function, key group, menu, status and error message. The bundled "Navigazione" profile is now "Navigation".
- README screenshots in light and dark appearance.

Upgrading from NagaController: quit it, delete `/Applications/NagaController.app`, then drag `OpenNaga.app` to Applications. macOS may ask again for Accessibility and Input Monitoring.

Apple Silicon build, ad-hoc signed, not notarized. On first launch right-click the app and choose Open, or run `xattr -dr com.apple.quarantine /Applications/OpenNaga.app`.
