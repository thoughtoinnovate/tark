#!/usr/bin/env python3
"""
Convert a static asciinema cast file to one with spread timestamps for animation.
TUI apps batch their output, so we need to spread events for animation.
"""
import json
import re
import sys

def make_animated(input_file, output_file=None, delay=0.1):
    """Spread timestamps in a cast file for animation."""
    if output_file is None:
        output_file = input_file.replace('.cast', '_animated.cast')
    
    with open(input_file, 'r') as f:
        lines = f.readlines()
    
    # Parse header
    header = json.loads(lines[0])
    header['idle_time_limit'] = 5
    new_lines = [json.dumps(header) + '\n']
    
    # Parse events and spread timestamps
    current_time = 0.0
    for line in lines[1:]:
        line = line.strip()
        if not line:
            continue
        match = re.match(r'\[([0-9.]+),\s*"([io])",\s*(.+)\]', line)
        if match:
            _, event_type, data = match.groups()
            current_time += delay
            new_lines.append(f'[{current_time:.6f}, "{event_type}", {data}]\n')
    
    with open(output_file, 'w') as f:
        f.writelines(new_lines)
    
    print(f"Created {output_file}")
    print(f"  Events: {len(new_lines)-1}")
    print(f"  Duration: {current_time:.1f}s")
    return output_file

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <input.cast> [output.cast] [delay]")
        sys.exit(1)
    
    input_file = sys.argv[1]
    output_file = sys.argv[2] if len(sys.argv) > 2 else None
    delay = float(sys.argv[3]) if len(sys.argv) > 3 else 0.1
    
    make_animated(input_file, output_file, delay)
