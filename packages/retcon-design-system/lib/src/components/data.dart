import 'package:flutter/material.dart';

import '../theme.dart';
import '../tokens.dart';

/// Column definition for [RetconTable].
class RetconTableColumn {
  const RetconTableColumn({
    required this.label,
    this.numeric = false,
    this.semanticsLabel,
  });

  final String label;
  final bool numeric;
  final String? semanticsLabel;
}

/// Row definition for [RetconTable].
class RetconTableRow {
  const RetconTableRow({
    required this.cells,
    this.semanticsLabel,
    this.enabled = true,
  });

  final List<String> cells;
  final String? semanticsLabel;
  final bool enabled;
}

/// Accessible data table with row selection and high-contrast borders.
class RetconTable extends StatelessWidget {
  const RetconTable({
    required this.columns,
    required this.rows,
    super.key,
    this.selectedRowIndex,
    this.onRowSelected,
    this.semanticsLabel = 'Data table',
  });

  final List<RetconTableColumn> columns;
  final List<RetconTableRow> rows;
  final int? selectedRowIndex;
  final ValueChanged<int>? onRowSelected;
  final String semanticsLabel;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Semantics(
      container: true,
      label: semanticsLabel,
      child: DecoratedBox(
        decoration: BoxDecoration(
          border: Border.all(color: retcon.borderColor),
        ),
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: DataTable(
            headingRowHeight: retcon.minimumTargetSize,
            dataRowMinHeight: retcon.minimumTargetSize,
            dataRowMaxHeight: retcon.minimumTargetSize + RetconSpacing.sm,
            headingTextStyle: Theme.of(context).textTheme.labelLarge,
            border: TableBorder.all(color: retcon.borderColor),
            showCheckboxColumn: false,
            columns: [
              for (final column in columns)
                DataColumn(
                  numeric: column.numeric,
                  label: Semantics(
                    header: true,
                    label: column.semanticsLabel ?? column.label,
                    child: Text(column.label),
                  ),
                ),
            ],
            rows: [
              for (var index = 0; index < rows.length; index++)
                _buildRow(context, retcon, index, rows[index]),
            ],
          ),
        ),
      ),
    );
  }

  DataRow _buildRow(
    BuildContext context,
    RetconTheme retcon,
    int index,
    RetconTableRow row,
  ) {
    final selected = selectedRowIndex == index;
    return DataRow(
      selected: selected,
      onSelectChanged: row.enabled && onRowSelected != null
          ? (_) => onRowSelected!(index)
          : null,
      color: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.selected)) {
          return retcon.highContrast
              ? retcon.focusColor.withValues(alpha: 0.25)
              : RetconColors.selection;
        }
        return null;
      }),
      cells: [
        for (var cellIndex = 0; cellIndex < row.cells.length; cellIndex++)
          DataCell(
            Semantics(
              label: row.semanticsLabel ?? row.cells[cellIndex],
              selected: selected,
              child: Text(row.cells[cellIndex]),
            ),
            showEditIcon: false,
          ),
      ],
    );
  }
}
