# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-opt::differential_evolution``: Differential evolution design optimizer."""

from __future__ import annotations

import _framework

_framework.add_alas_to_path()

from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.optimization.optimizer import DesignOptimizer  # noqa: E402


def generate() -> dict:
    config = ALASConfig()
    config.optimizer.solver.max_iterations = 2
    config.optimizer.solver.population_size = 1
    config.optimizer.solver.seed = 42
    config.optimizer.solver.seed_near_initial_design = True
    config.optimizer.solver.seed_perturbation_fraction = 0.05

    dv_init = DesignVector()
    opt = DesignOptimizer(config)
    result = opt.run(
        bounds=None,
        initial_design=dv_init,
    )

    best_dv = result.best_design
    best_arr = best_dv.to_array()

    return {
        "solver_settings": {
            "max_iterations": config.optimizer.solver.max_iterations,
            "population_size": config.optimizer.solver.population_size,
            "seed": config.optimizer.solver.seed,
            "seed_near_initial_design": config.optimizer.solver.seed_near_initial_design,
            "seed_perturbation_fraction": config.optimizer.solver.seed_perturbation_fraction,
        },
        "best_cost": float(result.best_cost),
        "best_design": [float(v) for v in best_arr],
        "n_evaluations": result.history.n_evaluations,
        "n_valid": result.history.n_valid,
    }


if __name__ == "__main__":
    _framework.write(
        "opt",
        "optimizer",
        generate(),
        description="Differential Evolution optimizer execution and result properties",
    )
