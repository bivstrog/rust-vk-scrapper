# RUST VK SCRAPPER
## Что за программа
API-приложение с интеграцией с БД, которое позволяет пулить информацию о посте в ВК

- ***rocket*** - WEB-фреймворк
- ***tokio-cron-scheduler*** - планировщик задач
- ***sqlx*** - ORM для работы с БД
- ***PostgreSQL*** - база данных

- ***.env*** - файл с конфигурацией приложения
- ***Docker-compose*** - файл для запуска БД в контейнере
- ***Makefile*** - файл для упрощения операций с приложением
## Как запустить
Для работы с приложением используется ***Makefile***

### Тесты можно запустить всегда:
```bash
make test
```
### Для запуска приложения:

Для запуска БД (если использовать docker-compose):
```bash
make db
```

Подготовить `.env` (вставить API-KEY в файл `.env.local`):
```bash
make create_local_env
```

Собрать и запустить приложение:
```bash
make run
```

После завершения приложения можно остановить и удалить БД:
```bash
make delete_db
```

### VK API:
- [Метод API](https://dev.vk.com/ru/method/wall.getById)
- [Сервисный ключ](https://dev.vk.com/ru/api/access-token/getting-started#Сервисный%20ключ%20доступа)

## Как использовать

Постановка задачи на парсинг:
```bash
curl --location 'http://127.0.0.1:8000/polling' \
--header 'Content-Type: application/json' \
--data '{
  "vk_link": "https://vk.com/wall-38894284_2277607",
  "prolong": true
}'
```

Пример ответа:
```json
{
    "scrapper_id": 2,
    "vk_id": "-38894284_2277607",
    "dt_parse_begin": "2026-02-25T21:52:04",
    "dt_parse_end": "2026-02-26T21:52:07"
}
```

Получение данных:
```bash
curl --location --request GET 'http://127.0.0.1:8000/polling?scrapper_id=2' \
--header 'Content-Type: application/json' \
--data '{
  "vk_link": "https://vk.com/wall-38894284_2277607",
  "prolong": true
}'
```

Пример ответа:
```json
{
    "scrapper_id": 2,
    "vk_id": "-38894284_2277607",
    "dt_parse_begin": "2026-02-25T21:52:04",
    "dt_parse_end": "2026-02-26T21:52:07",
    "dt_current": "2026-02-26T01:20:19",
    "data": [
        {
            "comments_count": 116,
            "likes_count": 162,
            "views_count": 160456,
            "reposts_count": 366,
            "info_time": "2026-02-25T21:52:30"
        },
        {
            "comments_count": 116,
            "likes_count": 162,
            "views_count": 160458,
            "reposts_count": 366,
            "info_time": "2026-02-25T21:53:00"
        }
    ]
}
```

## Места для доработок
- Улучшить постановку задач или их дропа (+ засунуть восстановление упавших задач в обязательно зарегестрированную таску)
- Унести задачи в отдельное приложение
- Добавить расширение милисекунд (будет лучше работать тест)
- Добавить явный Мок для планировщика задач и переписать тесты
- Добавить показ данных в виде картинки [plotters](https://docs.rs/plotters/latest/plotters/)