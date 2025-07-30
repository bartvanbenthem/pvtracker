# PVTracker
Kubernetes controller that logs the state of storage resources on a Kubernetes cluster. 

### upcoming release
Features in currently in development for the upcoming release:
* remove old logs based on a given retention time in days in the cr spec

## Build container
```bash
source ../00-ENV/env.sh
CVERSION="v0.3.2"

docker login ghcr.io -u bartvanbenthem -p $CR_PAT

docker build -t pvtracker:$CVERSION .

docker tag pvtracker:$CVERSION ghcr.io/bartvanbenthem/pvtracker:$CVERSION
docker push ghcr.io/bartvanbenthem/pvtracker:$CVERSION

# test image
docker run --rm -it --entrypoint /bin/sh pvtracker:$CVERSION

/# ls -l /usr/local/bin/pvtracker
/# /usr/local/bin/pvtracker
```

## Deploy CRD
```bash
kubectl apply -f ./config/crd/tracker.cndev.nl.yaml
# kubectl delete -f ./config/crd/tracker.cndev.nl.yaml
```

## Deploy Operator
```bash
kubectl apply -f ./config/manager/operator.yaml
# kubectl delete -f ./config/manager/operator.yaml
```

## Sample tracker
```bash
kubectl apply -f ./config/samples/tracker-example.yaml
kubectl apply -f ./config/samples/tracker2-example.yaml
kubectl describe volumetrackers.cndev.nl example-tracker
# kubectl delete -f ./config/samples/tracker-example.yaml
# kubectl delete -f ./config/samples/tracker2-example.yaml
```

## Test Watchers & Reconciler on Create Persistant Volumes
```bash
kubectl apply -f ./config/samples/test-pv.yaml
kubectl delete -f ./config/samples/test-pv.yaml
```

## CR Spec
```yaml
apiVersion: cndev.nl/v1beta1
kind: VolumeTracker
metadata:
  name: example-tracker
  namespace: default
  labels:
    app.kubernetes.io/name: volumetracker
    app.kubernetes.io/part-of: disaster-recovery-operator
  annotations:
    description: "Tracks persistent volume usage and logs it"
spec:
  clusterNameKey: cluster.x-k8s.io/cluster-name
  mountPath: pvtrackerlog
  retention: 14
```