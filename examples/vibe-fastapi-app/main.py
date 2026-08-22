from fastapi import FastAPI
import sqlite3

app = FastAPI(title="MOS FastAPI SQLite Demo")

def init_db():
    conn = sqlite3.connect("app.db")
    cur = conn.cursor()
    cur.execute("CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY, content TEXT)")
    cur.execute("INSERT OR IGNORE INTO notes VALUES (1, 'Persisted via Litestream S3 replica')")
    conn.commit()
    conn.close()

init_db()

@app.get("/")
def read_root():
    conn = sqlite3.connect("app.db")
    cur = conn.cursor()
    cur.execute("SELECT content FROM notes WHERE id=1")
    row = cur.fetchone()
    conn.close()
    return {"message": "Hello from MOS Python MicroVM", "note": row[0] if row else "None"}
