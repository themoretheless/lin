"""Counterbalance process roles using original, never rotated, case indices."""
from itertools import permutations
from collections import Counter

def role_order(round_number, original_case_index, role_ids):
    assert round_number>=1 and original_case_index>=0
    role_ids=tuple(role_ids);n=len(role_ids);assert len(set(role_ids))==n
    if n==3:
        designs=list(permutations(range(3)))
    elif n==6:
        # Six-treatment Williams design: all positions, ordered adjacencies and
        # pairwise precedence balance in each six-round block.
        first=(0,1,5,2,4,3)
        designs=[tuple((x+offset)%6 for x in first)for offset in range(6)]
    else:raise ValueError('Expected three or six roles')
    return tuple(role_ids[i]for i in designs[(round_number-1+original_case_index)%6])

def validate_orders(orders_by_case, role_ids, series_length=6):
    """Property checks independent of the scheduling formula."""
    role_ids=tuple(role_ids);n=len(role_ids);assert n in (3,6)and series_length==6
    assert orders_by_case
    for case,orders in orders_by_case.items():
        assert orders and len(orders)%series_length==0,(case,'incomplete series')
        for start in range(0,len(orders),series_length):
            block=orders[start:start+series_length]
            assert all(len(row)==n and set(row)==set(role_ids)for row in block),(case,'missing or duplicate role')
            for role in role_ids:
                counts=Counter(row.index(role)for row in block)
                assert all(counts[pos]==series_length//n for pos in range(n)),(case,'position imbalance',role,dict(counts))
            for left in role_ids:
                for right in role_ids:
                    if left==right:continue
                    assert sum(row.index(left)<row.index(right)for row in block)==series_length//2,(case,'precedence imbalance',left,right)
                    assert sum(any(a==left and b==right for a,b in zip(row,row[1:]))for row in block)==series_length//n,(case,'adjacency imbalance',left,right)
    return True

def orders_from_records(records,case_ids,role_ids,case_key,round_key='round',role_key='role',rounds=12):
    grouped={case:{rnd:[]for rnd in range(1,rounds+1)}for case in case_ids}
    for r in records:
        assert r[case_key]in grouped and r[round_key]in grouped[r[case_key]]
        grouped[r[case_key]][r[round_key]].append(r[role_key])
    orders={case:[tuple(by_round[rnd])for rnd in range(1,rounds+1)]for case,by_round in grouped.items()}
    validate_orders(orders,role_ids)
    return orders
