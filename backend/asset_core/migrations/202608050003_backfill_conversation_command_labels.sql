UPDATE conversation_parts
SET command_label = json_extract(metadata_json, '$.command_label')
WHERE command_label IS NULL
  AND metadata_json IS NOT NULL
  AND json_valid(metadata_json)
  AND json_type(metadata_json, '$.command_label') = 'text';

UPDATE web_record_parts
SET command_label = json_extract(metadata_json, '$.command_label')
WHERE command_label IS NULL
  AND metadata_json IS NOT NULL
  AND json_valid(metadata_json)
  AND json_type(metadata_json, '$.command_label') = 'text';
