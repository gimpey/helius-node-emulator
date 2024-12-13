#!/bin/bash

# Title: Repository Deployment
# Author: gimpey <gimpey@gimpey.com>
# GitHub: https://github.com/gimpey-com/db-gateway
# Description: Deploys the repository to the package registry.

if [ -z "$1" ]; then
    echo "Error: No version bump flag provided. Use --major, --minor, or --patch."
    exit 1
fi

# cleaning the package
yarn clean

# bump the version based on the flag
yarn bump-version $1

# the protos then need to be compiled to typescript
yarn build:protos

# building the package
yarn build

# # publish the package
yarn publish-package