"""Reproducible native CFD campaign using the same Rust runner as the GUI.

Example (paths are explicit; existing cases are never overwritten):
  python tools/cfd_validation_campaign.py --runner PATH --environment airfoil-cfd-environment.json \
    --manifest campaign.json --output out/cfd-campaign

Manifest is a list of {id, config, optional airfoil_dat}; config is a complete or
partial CfdStudyConfig. Outcomes, including rejection/failure, are retained.
This harness measures numerical robustness; it does not certify physical validity.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runner', type=Path, required=True)
    parser.add_argument('--environment', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    environment = json.loads(args.environment.read_text(encoding='utf-8-sig'))
    prefs = environment.get('openfoam', environment)
    env = os.environ.copy()
    # Prevent ambient example overrides from silently changing a manifest.
    for key in list(env):
        if key.startswith('ALAS_CFD_'):
            del env[key]
    env.update(ALAS_OPENFOAM_BIN=prefs['native_bin_dir'],
               ALAS_OPENFOAM_PROJECT=prefs['native_project_dir'],
               ALAS_GMSH=prefs.get('gmsh_executable', environment.get('gmsh_executable', 'gmsh')))
    manifest = json.loads(args.manifest.read_text(encoding='utf-8-sig'))
    args.output.mkdir(parents=True, exist_ok=True)
    records = []
    for item in manifest:
        if not item['id'] or Path(item['id']).name != item['id'] or item['id'] in ('.', '..'):
            raise ValueError('Each campaign id must be a single directory name')
        case = args.output / item['id']
        if any(path.exists() for path in (case, args.output / (item['id'] + '.config.json'),
                                           args.output / (item['id'] + '.log'))):
            raise FileExistsError(f'Refusing to overwrite evidence: {case}')
        config_file = args.output / (item['id'] + '.config.json')
        config_file.write_text(json.dumps(item['config'], indent=2), encoding='utf-8')
        run_env = dict(env, ALAS_CFD_CONFIG=str(config_file.resolve()))
        if item.get('airfoil_dat'):
            run_env['ALAS_CFD_AIRFOIL_DAT'] = str(Path(item['airfoil_dat']).resolve())
        started = time.monotonic()
        print(f'START {item["id"]}', flush=True)
        with (args.output / (item['id'] + '.log')).open('w', encoding='utf-8') as log:
            completed = subprocess.run([str(args.runner.resolve()), str(case.resolve())],
                                       env=run_env, stdout=log, stderr=subprocess.STDOUT)
        record = {'id': item['id'], 'returncode': completed.returncode,
                  'wall_seconds': time.monotonic() - started, 'case': str(case.resolve())}
        result_file = case / 'results.json'
        if result_file.exists():
            result = json.loads(result_file.read_text(encoding='utf-8'))
            record.update(outcome=result.get('outcome'), detail=result.get('status_detail'),
                          final_force=(result.get('forces') or [None])[-1],
                          mesh_quality=result.get('mesh_quality'),
                          mesh_qualification=result.get('mesh_qualification'))
            residuals = result.get('residuals', [])
            last = max((row['iteration'] for row in residuals), default=0)
            record['iterations'] = last
            record['final_initial_residuals'] = {}
            for row in residuals:
                if row['iteration'] == last:
                    field = row['field']
                    record['final_initial_residuals'][field] = max(
                        record['final_initial_residuals'].get(field, 0), row['initial'])
        else:
            log_text = (args.output / (item['id'] + '.log')).read_text(encoding='utf-8', errors='replace')
            record['detail'] = '\n'.join(log_text.splitlines()[-8:])
        records.append(record)
        (args.output / 'summary.json').write_text(json.dumps(records, indent=2), encoding='utf-8')
        print(f'END {item["id"]}: {record.get("outcome", "launch/preflight failure")}', flush=True)


if __name__ == '__main__':
    main()
