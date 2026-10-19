-- Universal Portable Anchors migration: upgrade legacy hardcoded paths to portable anchors
UPDATE profiles
SET payload = json_set(
    payload,
    '$.target_paths',
    json_array('@config/opencode/skills')
)
WHERE id = 'opencode'
  AND json_array_length(json_extract(payload, '$.target_paths')) = 1
  AND (
    json_extract(payload, '$.target_paths[0]') = '~/.config/opencode/skills'
    OR json_extract(payload, '$.target_paths[0]') = '~/.opencode/skills'
  );
