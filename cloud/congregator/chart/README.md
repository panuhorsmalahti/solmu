# Congregator Helm chart

Installs the Congregator web service and REST API. NVIDIA OpenShell and the
Kubernetes Agent Sandbox controller must already be installed in the cluster.
See [the cluster install guide](../../../docs/congregator.md).

Use `values.yaml` to set the OpenShell service URL, private CA Secret, optional
OpenShell service token, Solmu image, ingress, and resource requests.

From the root of a Solmu checkout, install it with:

```sh
helm upgrade --install congregator ./cloud/congregator/chart \
  --namespace congregator --create-namespace
```
