# Tab Color Rules

## Summary

Users can define rules that automatically assign a color to a tab based on its current working directory prefix, extending the existing tab naming rules with a color dimension. Rules are managed in the **Rules** tab of the Actions & Triggers panel.

## Behavior

### What is a tab color rule

1. A tab color rule has two fields: a **path prefix** and a **color**. When the working directory of a terminal starts with the prefix, the tab's color is automatically set to the chosen color.
2. Colors are chosen from the same set available in the right-click tab color menu: Red, Green, Blue, Yellow, Magenta, Cyan, White.
3. Rules apply on every working-directory change, the same way naming rules do.
4. When multiple rules match, the first matching rule wins (same tie-breaking as naming rules).
5. Color rules and naming rules are independent — both can match the same tab simultaneously.

### Managing color rules

6. The **Rules** tab of the Actions & Triggers panel gains a **"Tab Colors"** section below the existing **"Tab Names"** section, with its own **+ Add Color Rule** button.
7. Each color rule row shows the path prefix, a color swatch, and a delete button (trash icon).
8. Clicking **+ Add Color Rule** opens an inline form with:
   - **Path prefix** field — same behavior as naming rule prefix (supports `~`).
   - **Color** dropdown / selector — shows the named color options with a preview swatch.
   - **Save** and **Cancel** buttons.
9. Color rules are saved to `~/.warp/color_rules.toml` and reload on file change.

### Interaction with manual color changes

10. A manually set tab color (via the right-click color menu) takes precedence over any color rule. If the user has manually set a color (`SelectedTabColor::Color`), color rules do not override it.
11. If the user resets the tab color (via "Reset Color" in the right-click menu), the color rule fires again on the next CWD change.

### Edge cases

12. When no color rules match the current directory, the tab color is unchanged.
13. A rule with an empty prefix is treated as invalid and cannot be saved.
14. Rules take effect immediately when saved — the active tab's color updates if the rule matches its current directory.
