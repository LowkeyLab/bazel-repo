CREATE TYPE user_role AS ENUM ('member', 'admin');

CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role user_role NOT NULL DEFAULT 'member'
);

CREATE TABLE circles (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    creator_id INT NOT NULL REFERENCES users(id),
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE circle_members (
    circle_id INT NOT NULL REFERENCES circles(id) ON DELETE CASCADE,
    user_id INT NOT NULL REFERENCES users(id),
    clout INT NOT NULL DEFAULT 1000,
    PRIMARY KEY (circle_id, user_id)
);

CREATE TABLE contests (
    id SERIAL PRIMARY KEY,
    creator_id INT NOT NULL REFERENCES users(id),
    question TEXT NOT NULL,
    status TEXT NOT NULL,
    min_stake INT NOT NULL DEFAULT 10 CHECK (min_stake IN (10, 100, 1000)),
    consumption_rate DECIMAL(5, 2) NOT NULL DEFAULT 10.00,
    result_option_id INT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMP NOT NULL
);

CREATE TABLE contest_circles (
    contest_id INT NOT NULL REFERENCES contests(id) ON DELETE CASCADE,
    circle_id INT NOT NULL REFERENCES circles(id) ON DELETE CASCADE,
    PRIMARY KEY (contest_id, circle_id)
);

CREATE TABLE options (
    contest_id INT NOT NULL REFERENCES contests(id),
    option_id INT NOT NULL, -- 1, 2, 3...
    text TEXT NOT NULL,
    PRIMARY KEY (contest_id, option_id)
);

CREATE TABLE predictions (
    contest_id INT NOT NULL REFERENCES contests(id),
    user_id INT NOT NULL REFERENCES users(id),
    option_id INT NOT NULL,
    clout INT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    PRIMARY KEY (contest_id, user_id, option_id),
    FOREIGN KEY (contest_id, option_id) REFERENCES options(contest_id, option_id)
);