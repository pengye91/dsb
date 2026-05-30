// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React from 'react';
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  Button,
  VStack,
  Text,
  Progress,
  HStack,
  Spinner,
} from '@chakra-ui/react';

interface SandboxCreationProgressProps {
  isOpen: boolean;
  onClose: () => void;
  status: string;
  progress: number;
  error: string | null;
  isComplete: boolean;
}

export function SandboxCreationProgress({
  isOpen,
  onClose,
  status,
  progress,
  error,
  isComplete,
}: SandboxCreationProgressProps) {
  return (
    <Modal isOpen={isOpen} onClose={onClose} closeOnOverlayClick={false} closeOnEsc={false}>
      <ModalOverlay />
      <ModalContent>
        <ModalHeader>Creating Sandbox</ModalHeader>

        <ModalBody>
          <VStack align="stretch" spacing={4}>
            {error ? (
              <Text color="red.500">{error}</Text>
            ) : (
              <>
                <HStack>
                  {!isComplete && <Spinner size="sm" />}
                  <Text fontSize="sm" color="gray.600">
                    {status}
                  </Text>
                </HStack>

                {progress > 0 && (
                  <VStack align="stretch" spacing={2}>
                    <Progress
                      value={progress}
                      size="lg"
                      colorScheme="blue"
                      isAnimated={!isComplete}
                    />
                    <Text fontSize="xs" color="gray.500" textAlign="right">
                      {progress}%
                    </Text>
                  </VStack>
                )}

                {isComplete && !error && (
                  <Text color="green.500" fontWeight="bold">
                    ✓ Sandbox ready!
                  </Text>
                )}
              </>
            )}
          </VStack>
        </ModalBody>

        <ModalFooter>
          {error && (
            <Button colorScheme="blue" onClick={onClose}>
              Close
            </Button>
          )}
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}
