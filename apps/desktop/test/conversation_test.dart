import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/conversation/composer.dart';
import 'package:retcon_desktop/src/conversation/conversation_controller.dart';
import 'package:retcon_desktop/src/conversation/conversation_models.dart';
import 'package:retcon_desktop/src/conversation/conversation_panel.dart';
import 'package:retcon_desktop/src/conversation/message_list.dart';
import 'package:retcon_desktop/src/conversation/session_header.dart';
import 'package:retcon_desktop/src/core_client.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  group('ConversationController', () {
    late CoreClient core;
    late ConversationController controller;

    setUp(() {
      core = CoreClient(dataDirectory: Directory.systemTemp);
      controller = ConversationController(
        core: core,
        workingDirectory: r'C:\demo',
      );
    });

    tearDown(() {
      controller.dispose();
      core.dispose();
    });

    test('sendMessage requires a connected core', () async {
      await controller.sendMessage('Run tests');
      expect(controller.sessionState, ConversationSessionState.failed);
      expect(controller.sessionError, isNotNull);
    });

    test('approval events surface as pending approvals', () {
      controller.ingestEvent({
        'kind': 'approval.requested',
        'payload': {
          'id': 'a1',
          'title': 'Run shell command',
          'detail': 'npm test',
        },
      });

      expect(controller.pendingApprovals, hasLength(1));
      expect(controller.sessionState, ConversationSessionState.waitingApproval);
    });

    test('agent.line string payloads append assistant text', () {
      controller.ingestEvent({
        'kind': 'agent.line',
        'payload': {'id': 1, 'message': 'chunk-one'},
      });
      controller.ingestEvent({
        'kind': 'agent.line',
        'payload': {'id': 1, 'message': ' chunk-two'},
      });

      final assistant = controller.messages.lastWhere(
        (message) => message.role == ConversationMessageRole.assistant,
      );
      expect(assistant.text, 'chunk-one chunk-two');
    });

    test('tool events render tool cards', () {
      controller.ingestEvent({
        'kind': 'agent.event',
        'payload': {
          'kind': 'tool_started',
          'data': {'name': 'Read', 'input': 'README.md'},
        },
      });

      expect(controller.messages, hasLength(1));
      expect(controller.messages.first.toolName, 'Read');
    });
  });

  group('Conversation widgets', () {
    testWidgets('message list renders streaming assistant text', (
      tester,
    ) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: buildLunaDarkTheme(),
          home: Scaffold(
            body: MessageList(
              messages: const [
                ConversationMessage(
                  id: '1',
                  role: ConversationMessageRole.user,
                  text: 'Hello',
                ),
                ConversationMessage(
                  id: '2',
                  role: ConversationMessageRole.assistant,
                  text: 'Hi there',
                  streaming: true,
                ),
              ],
            ),
          ),
        ),
      );

      expect(find.text('Hello'), findsOneWidget);
      expect(find.text('Hi there'), findsOneWidget);
      expect(find.byType(LinearProgressIndicator), findsOneWidget);
    });

    testWidgets('composer toggles between send and stop', (tester) async {
      var sent = '';
      await tester.pumpWidget(
        MaterialApp(
          theme: buildLunaDarkTheme(),
          home: Scaffold(
            body: ConversationComposer(
              providers: const [
                ProviderOption(id: 'claude-code', label: 'Claude Code'),
              ],
              selectedProviderId: 'claude-code',
              selectedModel: 'default',
              isTurnActive: false,
              enabled: true,
              onSend: (value) => sent = value,
              onStop: () {},
              onProviderChanged: (_) {},
              onModelChanged: (_) {},
            ),
          ),
        ),
      );

      await tester.enterText(find.byType(TextField), 'Fix the tests');
      await tester.tap(find.text('Send'));
      await tester.pump();
      expect(sent, 'Fix the tests');

      await tester.pumpWidget(
        MaterialApp(
          theme: buildLunaDarkTheme(),
          home: Scaffold(
            body: ConversationComposer(
              providers: const [
                ProviderOption(id: 'claude-code', label: 'Claude Code'),
              ],
              selectedProviderId: 'claude-code',
              selectedModel: 'default',
              isTurnActive: true,
              enabled: true,
              onSend: (_) {},
              onStop: () {},
              onProviderChanged: (_) {},
              onModelChanged: (_) {},
            ),
          ),
        ),
      );

      expect(find.text('Stop'), findsOneWidget);
    });

    testWidgets('session header shows tokens and approvals', (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: buildLunaDarkTheme(),
          home: const Scaffold(
            body: SessionHeader(
              state: ConversationSessionState.waitingApproval,
              usage: TokenUsage(inputTokens: 12, outputTokens: 34),
              pendingApprovals: [
                PendingApproval(id: '1', title: 'Delete file'),
              ],
            ),
          ),
        ),
      );

      expect(find.text('46 tokens'), findsOneWidget);
      expect(find.text('1 approvals'), findsOneWidget);
      expect(find.text('Delete file'), findsOneWidget);
    });

    testWidgets('conversation panel renders composer and transcript', (
      tester,
    ) async {
      final core = CoreClient(dataDirectory: Directory.systemTemp);
      addTearDown(core.dispose);

      await tester.pumpWidget(
        MaterialApp(
          theme: buildLunaDarkTheme(),
          home: Scaffold(
            body: SizedBox(
              height: 640,
              width: 900,
              child: ConversationPanel(
                core: core,
                workingDirectory: r'C:\demo',
              ),
            ),
          ),
        ),
      );

      expect(
        find.text('Send a message to start an agent session.'),
        findsOneWidget,
      );
      expect(find.text('Send'), findsOneWidget);
      expect(find.text('Provider'), findsOneWidget);
    });
  });
}
