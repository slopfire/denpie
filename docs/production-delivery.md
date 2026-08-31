# Production delivery

Every successful push to `master` runs the full CI workflow. When the Docker
job succeeds, it publishes a prerelease named `continuous-<SHA>` containing
the immutable `slopsphere/denpie` image digest.

BRR's pull controller applies the newest such release. Do not retag an image or
edit server-side Compose to deploy a Denpie change. Commit the desired change
to `master`, wait for the GitHub workflow to pass, then inspect the controller
state if the running service has not updated.

The controller and its rollback rules live in the private
[`infra-state`](https://gitlab.com/DillerOFire/infra-state) repository under
`docs/continuous-deployment.md`.
