CREATE TABLE IF NOT EXISTS bukkens (
    bukken_id VARCHAR(255) NOT NULL,
    rent_normal VARCHAR(255) NOT NULL,
    rowspan INTEGER NOT NULL,
    PRIMARY KEY (bukken_id, rent_normal, rowspan)
);