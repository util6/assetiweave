pub(crate) const LOAD_PRINCIPAL: &str = r#"
SELECT id, kind, display_name, created_at, updated_at
FROM principals
WHERE id = ?1
"#;

pub(crate) const LOAD_TENANT: &str = r#"
SELECT id, slug, name, kind, status, created_at, updated_at
FROM tenants
WHERE id = ?1
"#;

pub(crate) const LOAD_TENANT_MEMBERSHIP: &str = r#"
SELECT tenant_id, principal_id, role, created_at, updated_at
FROM tenant_memberships
WHERE tenant_id = ?1 AND principal_id = ?2
"#;

pub(crate) const LIST_TENANTS_BY_PRINCIPAL: &str = r#"
SELECT tenant.id, tenant.slug, tenant.name, tenant.kind, tenant.status,
       tenant.created_at, tenant.updated_at
FROM tenants tenant
JOIN tenant_memberships membership ON membership.tenant_id = tenant.id
WHERE membership.principal_id = ?1
ORDER BY tenant.name ASC, tenant.id ASC
"#;

pub(crate) const LOAD_ACTIVE_TENANT_ID: &str = r#"
SELECT active_tenant_id
FROM tenant_state
WHERE principal_id = ?1
"#;

pub(crate) const UPSERT_LOCAL_PRINCIPAL: &str = r#"
INSERT INTO principals (id, kind, display_name, created_at, updated_at)
VALUES ('local', 'local', 'Local User', ?1, ?1)
ON CONFLICT(id) DO NOTHING
"#;

pub(crate) const UPSERT_DEFAULT_TENANT: &str = r#"
INSERT INTO tenants (id, slug, name, kind, status, created_at, updated_at)
VALUES ('default', 'default', 'Default', 'local_workspace', 'active', ?1, ?1)
ON CONFLICT(id) DO NOTHING
"#;

pub(crate) const UPSERT_DEFAULT_TENANT_MEMBERSHIP: &str = r#"
INSERT INTO tenant_memberships (tenant_id, principal_id, role, created_at, updated_at)
VALUES ('default', 'local', 'owner', ?1, ?1)
ON CONFLICT(tenant_id, principal_id) DO NOTHING
"#;

pub(crate) const UPSERT_LOCAL_TENANT_STATE: &str = r#"
INSERT INTO tenant_state (principal_id, active_tenant_id, updated_at)
VALUES ('local', 'default', ?1)
ON CONFLICT(principal_id) DO NOTHING
"#;

pub(crate) const UPDATE_ACTIVE_TENANT: &str = r#"
UPDATE tenant_state
SET active_tenant_id = ?2, updated_at = ?3
WHERE principal_id = ?1
"#;

pub(crate) const INSERT_TENANT: &str = r#"
INSERT INTO tenants (id, slug, name, kind, status, created_at, updated_at)
VALUES (?1, ?2, ?3, 'local_workspace', 'active', ?4, ?4)
"#;

pub(crate) const INSERT_TENANT_MEMBERSHIP: &str = r#"
INSERT INTO tenant_memberships (tenant_id, principal_id, role, created_at, updated_at)
VALUES (?1, ?2, 'owner', ?3, ?3)
"#;

pub(crate) const GET_NAVIGATION_STATE: &str = r#"
SELECT active_rail_id, active_header_tab_id, active_sub_nav_id
FROM navigation_state
WHERE tenant_id = ?1 AND id = 'default'
"#;

pub(crate) const LIST_RAIL_MENU_ITEMS: &str = r#"
SELECT id, label, label_zh, label_en, icon, scope, enabled, position
FROM rail_menu_items
ORDER BY sort_order ASC, id ASC
"#;

pub(crate) const LIST_HEADER_TAB_ITEMS: &str = r#"
SELECT id, label, label_zh, label_en, asset_kind, enabled
FROM header_tab_items
ORDER BY sort_order ASC, id ASC
"#;

pub(crate) const LIST_SUB_NAV_ITEMS: &str = r#"
SELECT parent_tab_id, id, label, label_zh, label_en, route_key, enabled
FROM sub_nav_items
ORDER BY parent_tab_id ASC, sort_order ASC, id ASC
"#;

pub(crate) const UPSERT_NAVIGATION_STATE: &str = r#"
INSERT INTO navigation_state (tenant_id, id, active_rail_id, active_header_tab_id, active_sub_nav_id)
VALUES (?1, 'default', ?2, ?3, ?4)
ON CONFLICT(tenant_id, id) DO UPDATE SET
    active_rail_id = excluded.active_rail_id,
    active_header_tab_id = excluded.active_header_tab_id,
    active_sub_nav_id = excluded.active_sub_nav_id
"#;

pub(crate) const UPSERT_RAIL_MENU_ITEM: &str = r#"
INSERT INTO rail_menu_items (id, label, label_zh, label_en, icon, scope, enabled, position, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
ON CONFLICT(id) DO UPDATE SET
    label = excluded.label,
    label_zh = excluded.label_zh,
    label_en = excluded.label_en,
    icon = excluded.icon,
    scope = excluded.scope,
    enabled = excluded.enabled,
    position = excluded.position,
    sort_order = excluded.sort_order
"#;

pub(crate) const UPSERT_HEADER_TAB_ITEM: &str = r#"
INSERT INTO header_tab_items (id, label, label_zh, label_en, asset_kind, enabled, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(id) DO UPDATE SET
    label = excluded.label,
    label_zh = excluded.label_zh,
    label_en = excluded.label_en,
    asset_kind = excluded.asset_kind,
    enabled = excluded.enabled,
    sort_order = excluded.sort_order
"#;

pub(crate) const UPSERT_SUB_NAV_ITEM: &str = r#"
INSERT INTO sub_nav_items (parent_tab_id, id, label, label_zh, label_en, route_key, enabled, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
ON CONFLICT(parent_tab_id, id) DO UPDATE SET
    label = excluded.label,
    label_zh = excluded.label_zh,
    label_en = excluded.label_en,
    route_key = excluded.route_key,
    enabled = excluded.enabled,
    sort_order = excluded.sort_order
"#;

pub(crate) const LIST_APP_SHORTCUTS: &str = r#"
SELECT shortcut.profile_id, shortcut.display_icon, shortcut.icon_svg, shortcut.accent_color,
       shortcut.enabled, profile.payload
FROM app_shortcut_items shortcut
JOIN profiles profile ON profile.tenant_id = shortcut.tenant_id AND profile.id = shortcut.profile_id
WHERE shortcut.tenant_id = ?1 AND shortcut.enabled = 1
ORDER BY shortcut.sort_order ASC, shortcut.profile_id ASC
"#;

pub(crate) const LIST_APP_SHORTCUT_SETTINGS: &str = r#"
SELECT profile.id, profile.payload, shortcut.display_icon, shortcut.icon_svg, shortcut.accent_color,
       COALESCE(shortcut.enabled, 1) AS enabled,
       COALESCE(shortcut.sort_order, 9999) AS sort_order
FROM profiles profile
LEFT JOIN app_shortcut_items shortcut
    ON shortcut.tenant_id = profile.tenant_id AND shortcut.profile_id = profile.id
WHERE profile.tenant_id = ?1
ORDER BY sort_order ASC, profile.id ASC
"#;

pub(crate) const UPSERT_APP_SHORTCUT: &str = r#"
INSERT INTO app_shortcut_items (
    tenant_id, profile_id, display_icon, icon_svg, accent_color, enabled, sort_order
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(tenant_id, profile_id) DO UPDATE SET
    display_icon = excluded.display_icon,
    icon_svg = excluded.icon_svg,
    accent_color = excluded.accent_color,
    enabled = excluded.enabled,
    sort_order = excluded.sort_order
"#;
