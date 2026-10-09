UPDATE profiles
SET payload = json_set(
    payload,
    '$.target_paths',
    json_array('@config/Cursor/skills')
)
WHERE id = 'cursor'
  AND json_array_length(json_extract(payload, '$.target_paths')) = 1
  AND json_extract(payload, '$.target_paths[0]') = '~/Library/Application Support/Cursor/skills';
