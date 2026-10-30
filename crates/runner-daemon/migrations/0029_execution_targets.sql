ALTER TABLE roles ADD COLUMN execution_target_json TEXT NOT NULL DEFAULT '{"kind":"native"}';
ALTER TABLE missions ADD COLUMN execution_target_json TEXT NOT NULL DEFAULT '{"kind":"native"}';
ALTER TABLE projects ADD COLUMN execution_target_json TEXT NOT NULL DEFAULT '{"kind":"native"}';
