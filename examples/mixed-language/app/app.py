from pathlib import Path

generated = Path(__file__).with_name("generated.txt")
print(generated.read_text(encoding="utf-8"), end="")
