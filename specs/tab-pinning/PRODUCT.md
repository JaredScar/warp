# Tab Pinning

## Summary

Users can pin any terminal tab so it is protected from bulk-close operations and workspace switches. Pinned tabs display a visual indicator and can be individually unpinned at any time.

## Behavior

### Pinning and unpinning

1. Right-clicking a tab in the tab bar or sidebar shows a **"Pin Tab"** menu item. If the tab is already pinned, the item reads **"Unpin Tab"** instead.
2. Clicking "Pin Tab" marks the tab as pinned immediately. Clicking "Unpin Tab" removes the pin.

### Visual indicator

3. A pinned tab displays a **📌 pin icon** to the left of its title in the horizontal tab bar and in the sidebar tab list.
4. The icon is visible at all times — not only on hover.

### Protection from bulk-close

5. **Close All Terminals** skips all pinned tabs. If only pinned tabs remain, no tabs are closed (the operation is a no-op).
6. **Close Other Tabs** (right-click → "Close other tabs") skips pinned tabs — only non-pinned tabs other than the target are closed.
7. **Close Tabs to the Right / Below** skips pinned tabs in the direction being closed.
8. **Close Non-Active Tabs** skips pinned tabs.
9. Individually closing a pinned tab via "Close tab" or its close button works normally — the pin does not block direct, deliberate close.

### Persistence

10. When a workspace is saved, each tab's pinned state is included in the snapshot.
11. When a workspace is restored, tabs are restored with their saved pinned state.

### Edge cases

12. Moving a pinned tab (Move Tab Left / Right / Up / Down) works normally — the pin travels with the tab.
13. Pinned tabs can be added to groups, renamed, and have their color changed the same as any other tab.
14. There is no cap on the number of pinned tabs.
