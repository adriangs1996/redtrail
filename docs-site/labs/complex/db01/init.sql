CREATE DATABASE IF NOT EXISTS techpulse_prod;
USE techpulse_prod;

CREATE TABLE users (
    id INT AUTO_INCREMENT PRIMARY KEY,
    username VARCHAR(64) NOT NULL,
    email VARCHAR(128) NOT NULL,
    password VARCHAR(128) NOT NULL,
    role VARCHAR(32) NOT NULL DEFAULT 'user',
    department VARCHAR(64),
    last_login DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO users (username, email, password, role, department, last_login) VALUES
('jsmith',      'jsmith@techpulse.local',      'Welc0me!2024',                    'user',        'Engineering',  '2026-03-18 09:14:22'),
('agarcia',     'agarcia@techpulse.local',      '$2b$12$LJ3m5Zq2v7Hx8jKlMnOpQeRtUvWxYz', 'user', 'Marketing',    '2026-03-17 16:45:01'),
('devops',      'devops@techpulse.local',       'TechPulse2024!',                  'service',     'Operations',   '2026-03-19 22:30:00'),
('tp_backup',   'backup@techpulse.local',       'Backup#Str0ng99',                 'service',     'IT',           '2026-03-15 03:00:00'),
('dc01admin',   'dc01admin@techpulse.local',    'Domain@dmin2024!',                'domain_admin','IT Security',  '2026-03-19 11:20:33'),
('mwilson',     'mwilson@techpulse.local',      'Summer2025!',                     'user',        'Sales',        '2026-03-16 08:55:12'),
('klee',        'klee@techpulse.local',         '$2b$12$9xPqRsTuVwXyZaBcDeFgHiJkLmNoPq', 'admin', 'Engineering', '2026-03-18 14:02:44');

CREATE TABLE sessions (
    id INT AUTO_INCREMENT PRIMARY KEY,
    user_id INT NOT NULL,
    token VARCHAR(256) NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME,
    FOREIGN KEY (user_id) REFERENCES users(id)
);

INSERT INTO sessions (user_id, token, created_at, expires_at) VALUES
(3, 'srv-tok-devops-a1b2c3d4e5f6', '2026-03-19 22:30:00', '2026-03-20 22:30:00'),
(5, 'adm-tok-dc01-f7e8d9c0b1a2',  '2026-03-19 11:20:33', '2026-03-19 23:20:33');

CREATE TABLE config (
    id INT AUTO_INCREMENT PRIMARY KEY,
    setting_key VARCHAR(128) NOT NULL,
    setting_value VARCHAR(256) NOT NULL,
    description VARCHAR(256)
);

INSERT INTO config (setting_key, setting_value, description) VALUES
('db_version',   '8.0.36',           'MySQL server version'),
('backup_path',  '/var/backups/db',   'Nightly backup location'),
('api_endpoint', 'http://app01:8080', 'Internal API endpoint'),
('dc01_host',    '10.0.1.10',         'Domain controller address');

CREATE USER IF NOT EXISTS 'tp_app'@'%' IDENTIFIED BY 'Pr0d-DB#Acc3ss!';
GRANT SELECT, INSERT, UPDATE, DELETE, FILE ON *.* TO 'tp_app'@'%';
CREATE USER IF NOT EXISTS 'tp_app'@'localhost' IDENTIFIED BY 'Pr0d-DB#Acc3ss!';
GRANT SELECT, INSERT, UPDATE, DELETE, FILE ON *.* TO 'tp_app'@'localhost';
FLUSH PRIVILEGES;
