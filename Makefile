db:  ##@Database Create database with docker-compose
	docker compose -f docker-compose.yml up -d

run:
	cp .env.local .env && cargo run

delete_db:
	docker stop vk_scrapper-db-1 && docker rm vk_scrapper-db-1

create_local_env:
	cp .env.example .env.local

test:
	cp .env.test .env && docker stop vk_scrapper-db-1 && docker rm vk_scrapper-db-1 && docker compose -f docker-compose.yml up -d && cargo test -- --test-threads=1