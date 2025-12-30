CREATE TABLE users (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE
);

CREATE TABLE circles (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    invite_code TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE circle_members (
    circle_id UUID NOT NULL REFERENCES circles(id),
    user_id UUID NOT NULL REFERENCES users(id),
    clout INT NOT NULL DEFAULT 1000,
    PRIMARY KEY (circle_id, user_id)
);

CREATE TABLE contests (
    id UUID PRIMARY KEY,
    circle_id UUID NOT NULL REFERENCES circles(id),
    creator_id UUID NOT NULL REFERENCES users(id),
    question TEXT NOT NULL,
    status TEXT NOT NULL,
    result_option_id INT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMP NOT NULL
);

CREATE TABLE options (
    contest_id UUID NOT NULL REFERENCES contests(id),
    option_id INT NOT NULL, -- 1, 2, 3...
    text TEXT NOT NULL,
    PRIMARY KEY (contest_id, option_id)
);

CREATE TABLE predictions (
    id UUID PRIMARY KEY,
    contest_id UUID NOT NULL REFERENCES contests(id),
    user_id UUID NOT NULL REFERENCES users(id),
    option_id INT NOT NULL,
    clout INT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    FOREIGN KEY (contest_id, option_id) REFERENCES options(contest_id, option_id)
);