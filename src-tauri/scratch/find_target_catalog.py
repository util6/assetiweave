import subprocess
import glob

output = subprocess.check_output(["git", "grep", "-n", "TargetCatalog", "src-tauri/src"], text=True)
print(output)
