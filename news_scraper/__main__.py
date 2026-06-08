try:
    from .main import main
except ImportError:
    from news_scraper.main import main

if __name__ == "__main__":
    raise SystemExit(main())