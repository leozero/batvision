-- Mobile data becomes a mode (D68): off, light versions only, or all qualities.
ALTER TABLE device_settings ADD COLUMN mobile_data_mode TEXT NOT NULL DEFAULT 'off'
    CHECK (mobile_data_mode IN ('off', 'light', 'all'));
UPDATE device_settings SET mobile_data_mode = 'light' WHERE mobile_data = 1;
ALTER TABLE device_settings DROP COLUMN mobile_data;
ALTER TABLE device_settings RENAME COLUMN mobile_data_mode TO mobile_data;

-- Download only while the device charges (D69). Off by default.
ALTER TABLE device_settings ADD COLUMN charging_only INTEGER NOT NULL DEFAULT 0;
