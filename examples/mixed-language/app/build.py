from pathlib import Path
import py_compile
import subprocess
import sys

output_dir = Path("build")
output_dir.mkdir(exist_ok=True)
generated = subprocess.check_output([sys.argv[1]])
(output_dir / "generated.txt").write_bytes(generated)
py_compile.compile("app.py", cfile=str(output_dir / "app.pyc"), doraise=True)
