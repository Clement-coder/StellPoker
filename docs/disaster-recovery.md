# Disaster Recovery Plan - StellPoker

## Incident Recovery Workflow
The following steps outline the order of operations for system recovery:
1. **Contract Pause**: Halt movement.
2. **Coordinator/MPC Shift**: Route around failure.
3. **DB Restore**: Validate state.
4. **Frontend Rollback**: Normalize user experience.



## 1. Emergency Pause (Contract)
- **Trigger**: Detected abnormal transaction volume.
- **Action**: Execute pause() on core contracts via emergency multi-sig.

## 2. Coordinator Failover
- **Process**: Switch traffic to standby instances if primary becomes unresponsive.

## 3. MPC Node Replacement
- **Process**: Revoke keys, provision fresh node, and resync threshold shard set.

## 4. Database Restore (PITR)
- **Process**: Initiate Point-in-Time Recovery (PITR) to last stable state (RPO < 5 minutes).

## 5. Frontend Rollback
- **Process**: Revert to previous known-good deployment environment.

## 6. Communication Template
- **Subject**: Urgent Service Update
- **Body**: StellPoker is under emergency maintenance. All funds are secure. Restoration in progress. Follow our status page.
