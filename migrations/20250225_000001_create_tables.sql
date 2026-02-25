-- Создание расширения для exclusion constraint
CREATE EXTENSION IF NOT EXISTS btree_gist;

-- Создание таблицы POST
CREATE TABLE IF NOT EXISTS POST (
    id SERIAL PRIMARY KEY,
    vk_id VARCHAR(255) NOT NULL,
    dt_parse_begin TIMESTAMP,
    dt_parse_end TIMESTAMP,
    CONSTRAINT no_overlapping_periods EXCLUDE USING gist (
        vk_id WITH =,
        tsrange(dt_parse_begin, dt_parse_end) WITH &&
    )
);

-- Создание таблицы POST_INFO
CREATE TABLE IF NOT EXISTS POST_INFO (
    id SERIAL PRIMARY KEY,
    post_id INTEGER NOT NULL,
    likes_count INTEGER NOT NULL DEFAULT 0,
    comments_count INTEGER NOT NULL DEFAULT 0,
    reposts_count INTEGER NOT NULL DEFAULT 0,
    views_count INTEGER NOT NULL DEFAULT 0,
    info_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    -- Определение внешнего ключа
    CONSTRAINT fk_post_info_post FOREIGN KEY (post_id) 
        REFERENCES POST(id) ON DELETE CASCADE
);

-- Создание индекса для ускорения запросов по post_id
CREATE INDEX IF NOT EXISTS idx_post_info_post_id ON POST_INFO(post_id);

-- Создание индекса для ускорения запросов по vk_id
CREATE INDEX IF NOT EXISTS idx_post_vk_id ON POST(vk_id);