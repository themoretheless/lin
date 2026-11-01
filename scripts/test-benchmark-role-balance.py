from benchmark_role_balance import role_order,validate_orders
checks=[]
for n in [3,6]:
 roles=tuple(f'role-{i}'for i in range(n))
 for count in [1,4,8,20,21,24,67]:
  emitted={j:[]for j in range(count)}
  for rnd in range(1,13):
   original=list(range(count));rotation=rnd%count
   for original_index in original[rotation:]+original[:rotation]:emitted[original_index].append(role_order(rnd,original_index,roles))
  assert validate_orders(emitted,roles);checks.append((n,count))
# The actual previous cancellation bug must be rejected, not merely reproduced.
roles=('baseline','control','candidate');bad={j:[]for j in range(24)}
for rnd in range(1,13):
 cases=list(range(24));rotation=rnd%24
 for position,j in enumerate(cases[rotation:]+cases[:rotation]):
  shift=(rnd+position)%3;bad[j].append(roles[shift:]+roles[:shift])
try:validate_orders(bad,roles)
except AssertionError:pass
else:raise AssertionError('Old fixed-within-case role plan accepted')
print('PASS 14 complete balanced schedules; old 24-case cancellation plan rejected')
