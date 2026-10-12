-- NULL device ID means "follow the macOS system default input".
-- The name is retained only so Settings can label a saved device while it is disconnected.
ALTER TABLE product_settings ADD COLUMN microphone_device_id TEXT
    CHECK (microphone_device_id IS NULL OR length(microphone_device_id) BETWEEN 1 AND 512);
ALTER TABLE product_settings ADD COLUMN microphone_device_name TEXT
    CHECK (microphone_device_name IS NULL OR length(microphone_device_name) BETWEEN 1 AND 512);
