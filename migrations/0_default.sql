-- SQL default migration with simple tables.

CREATE TABLE user_info (
       user_name TEXT NOT NULL,
       api_key TEXT NOT NULL,

       PRIMARY KEY (api_key)
);

CREATE TABLE instance_info (
       container_id TEXT NOT NULL,
       instance_name TEXT NOT NULL,
       api_key TEXT NOT NULL,
       proxied_port INT NOT NULL,

       PRIMARY KEY (`instance_name`)
);
