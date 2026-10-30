APP_NAME := clean-projects
FRONTEND_DIR := frontend
BACKEND_DIR := backend
STATIC_DIR := $(BACKEND_DIR)/static
DIST_DIR := $(FRONTEND_DIR)/dist

.PHONY: all frontend embed build test clean

all: build

frontend:
	cd $(FRONTEND_DIR) && npm ci && npm run build

embed: frontend
	mkdir -p $(STATIC_DIR)
	find $(STATIC_DIR) -mindepth 1 -depth -delete
	cp -R $(DIST_DIR)/. $(STATIC_DIR)/

build: embed
	cd $(BACKEND_DIR) && go build -o ../$(APP_NAME) .

test:
	cd $(BACKEND_DIR) && go test ./...
	cd $(FRONTEND_DIR) && npm run build

clean:
	find $(STATIC_DIR) -mindepth 1 -depth -delete
	rm -rf $(DIST_DIR) $(APP_NAME)

