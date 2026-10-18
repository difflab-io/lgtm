"""Isolated real editor/daemon E2E runner; does not create remote repositories."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "common"))
from github_server import Github

SUITE = Path(__file__).resolve().parents[1]
ROOT = SUITE.parents[1]
CACHE = SUITE / '.cache'


def install():
    CACHE.mkdir(exist_ok=True)
    for name, pin in json.loads((SUITE / 'dependencies.lock.json').read_text()).items():
        dest = CACHE / name
        if not dest.exists():
            subprocess.run(['git', 'clone', '--no-checkout', pin['repository'], str(dest)], check=True)
        subprocess.run(['git', '-C', str(dest), 'fetch', 'origin', pin['commit']], check=True)
        subprocess.run(['git', '-C', str(dest), 'checkout', '--detach', pin['commit']], check=True)
        actual = subprocess.check_output(['git', '-C', str(dest), 'rev-parse', 'HEAD'], text=True).strip()
        if actual != pin['commit']:
            raise RuntimeError(f'Dependency revision mismatch: {name}')
    subprocess.run(['nvim', '--headless', '-u', str(SUITE / 'minimal_init.lua'), '-c',
                    "lua assert(require('codediff.core.diff').compute_diff({'a'}, {'b'}))", '-c', 'qa!'], check=True,
                   env={**os.environ, 'LGTM_REPO_ROOT': str(ROOT)})


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--install', action='store_true')
    args = parser.parse_args()
    if args.install:
        install()
        return
    pins = json.loads((SUITE / 'dependencies.lock.json').read_text())
    for name, pin in pins.items():
        dest = CACHE / name
        if not dest.exists():
            raise RuntimeError('Missing test dependencies; run this suite install task first')
        actual = subprocess.check_output(['git', '-C', str(dest), 'rev-parse', 'HEAD'], text=True).strip()
        if actual != pin['commit']:
            raise RuntimeError(f'Wrong dependency revision: {name}')
    binary = Path(os.environ.get('LGTM_E2E_BINARY', ROOT / 'target/debug/lgtm'))
    if not binary.exists():
        raise RuntimeError('Build lgtm CLI first through the project Mise build task')
    with tempfile.TemporaryDirectory(prefix='lgtm-nvim-e2e-') as temp:
        base = Path(temp)
        fixture = base / 'workspace'
        fixture.mkdir()
        (fixture / 'notes.txt').write_text('one\ntwo\nthree\n')
        (fixture / '.gitignore').write_text('.difflab\nignored/\n')
        subprocess.run(['git', 'init', '-q', str(fixture)], check=True)
        subprocess.run(['git', '-C', str(fixture), 'add', '.'], check=True)
        subprocess.run(['git', '-C', str(fixture), '-c', 'user.name=E2E', '-c', 'user.email=e2e@example.test', 'commit', '-qm', 'fixture'], check=True)
        (fixture / 'notes.txt').write_text('one\nchanged\nthree\n')
        plans = base / 'plans' / 'specific-plan'
        plans.mkdir(parents=True)
        (plans / 'PLAN.md').write_text('# Plan\nReview ignored content\n')
        (plans / 'SECOND.md').write_text('# Second\nAnother review file\n')
        (fixture / '.difflab').symlink_to(plans.parent, target_is_directory=True)
        home = base / 'home'
        env = {**os.environ, 'LGTM_HOME': str(home), 'LGTM_REPO_ROOT': str(ROOT),
               'LGTM_E2E_WORKSPACE': str(fixture), 'PATH': str(binary.parent) + os.pathsep + os.environ['PATH'], 'XDG_CONFIG_HOME': str(base / 'config'),
               'XDG_DATA_HOME': str(base / 'data'), 'XDG_STATE_HOME': str(base / 'state'),
               'XDG_CACHE_HOME': str(base / 'cache')}
        github = Github()
        env.update({'LGTM_GITHUB_TEST_MODE': '1', 'LGTM_GITHUB_TEST_API_URL': github.endpoint_url, 'LGTM_E2E_GITHUB': github.endpoint_url})
        with (base / 'daemon.log').open('w+') as log:
            process = subprocess.Popen([str(binary), 'up', '--foreground', '--no-open', '--port', '0'], env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 20
                while not (home / 'server.port').exists():
                    if process.poll() is not None or time.monotonic() > deadline:
                        log.seek(0)
                        raise RuntimeError('Daemon did not start: ' + log.read())
                    time.sleep(.05)
                for case in ['workflow', 'persistence', 'github_review', 'health', 'transport', 'notifications']:
                    subprocess.run(['nvim', '--headless', '-u', str(SUITE / 'minimal_init.lua'), '-c', f"lua vim.g.lgtm_case='{case}'", '-c', 'luafile ' + str(SUITE / 'common' / 'runner.lua')], env=env, cwd=fixture, check=True, timeout=120)
                if github.errors:
                    raise RuntimeError("Unexpected GitHub fixture requests: " + repr(github.errors))
            finally:
                github.close()
                if github.errors:
                    print('GitHub fixture errors:', github.errors)
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


if __name__ == '__main__':
    main()
