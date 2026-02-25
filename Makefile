db:  ##@Database Create database with docker-compose
	docker compose -f docker-compose.yml up -d

run:
	cargo run

delete_db:
	docker stop vk_scrapper-db-1 && docker rm vk_scrapper-db-1

test:
	docker stop vk_scrapper-db-1 && docker rm vk_scrapper-db-1 && docker compose -f docker-compose.yml up -d && cargo test